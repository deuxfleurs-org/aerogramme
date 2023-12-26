use crate::storage::*;
use serde::Serialize;
use aws_sdk_s3::{
    self as s3,
    error::SdkError,
    operation::get_object::GetObjectError,
};

#[derive(Clone, Debug, Serialize)]
pub struct GarageConf {
    pub region: String,
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

#[derive(Clone, Debug)]
pub struct GarageBuilder {
    conf: GarageConf,
    unicity: Vec<u8>,
}

impl GarageBuilder {
    pub fn new(conf: GarageConf) -> anyhow::Result<Arc<Self>> {
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.append(&mut rmp_serde::to_vec(&conf)?);
        Ok(Arc::new(Self { conf, unicity }))
    } 
}

#[async_trait]
impl IBuilder for GarageBuilder {
    async fn build(&self) -> Result<Store, StorageError> {
        let s3_creds = s3::config::Credentials::new(
            self.conf.aws_access_key_id.clone(), 
            self.conf.aws_secret_access_key.clone(), 
            None, 
            None, 
            "aerogramme"
        );

        let s3_config = aws_config::from_env()
            .region(aws_config::Region::new(self.conf.region.clone()))
            .credentials_provider(s3_creds)
            .endpoint_url(self.conf.s3_endpoint.clone())
            .load()
            .await;
        let s3_client = aws_sdk_s3::Client::new(&s3_config);

        let k2v_config = k2v_client::K2vClientConfig {
	        endpoint: self.conf.k2v_endpoint.clone(),
		    region: self.conf.region.clone(),
		    aws_access_key_id: self.conf.aws_access_key_id.clone(),
		    aws_secret_access_key: self.conf.aws_secret_access_key.clone(),
		    bucket: self.conf.bucket.clone(),
		    user_agent: None,
        };

        let k2v_client = match k2v_client::K2vClient::new(k2v_config) {
            Err(e) => {
                tracing::error!("unable to build k2v client: {}", e);
                return Err(StorageError::Internal);
            }
            Ok(v) => v,
        };

        Ok(Box::new(GarageStore { 
            bucket: self.conf.bucket.clone(),
            s3: s3_client,
            k2v: k2v_client,
        }))
    }
    fn unique(&self) -> UnicityBuffer {
        UnicityBuffer(self.unicity.clone())
    }
}

pub struct GarageStore {
    bucket: String,
    s3: s3::Client,
    k2v: k2v_client::K2vClient,
}

fn causal_to_row_val(row_ref: RowRef, causal_value: k2v_client::CausalValue) -> RowVal {
    let new_row_ref = row_ref.with_causality(causal_value.causality.into());
    let row_values = causal_value.value.into_iter().map(|k2v_value| match k2v_value {
        k2v_client::K2vValue::Tombstone => Alternative::Tombstone,
        k2v_client::K2vValue::Value(v) => Alternative::Value(v),
    }).collect::<Vec<_>>();

    RowVal { row_ref: new_row_ref, value: row_values }
}

#[async_trait]
impl IStore for GarageStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError> {
        let (pk_list, batch_op) = match select {
            Selector::Range { shard, sort_begin, sort_end } => (
                vec![shard.to_string()],
                vec![k2v_client::BatchReadOp {
                    partition_key: shard,
                    filter: k2v_client::Filter {
                        start: Some(sort_begin),
                        end: Some(sort_end),
                        ..k2v_client::Filter::default()
                    },
                    ..k2v_client::BatchReadOp::default()
                }]
            ),
            Selector::List(row_ref_list) => (
                row_ref_list.iter().map(|row_ref| row_ref.uid.shard.to_string()).collect::<Vec<_>>(),
                row_ref_list.iter().map(|row_ref| k2v_client::BatchReadOp {
                    partition_key: &row_ref.uid.shard,
                    filter: k2v_client::Filter {
                        start: Some(&row_ref.uid.sort),
                        ..k2v_client::Filter::default()
                    },
                    single_item: true,
                    ..k2v_client::BatchReadOp::default()
                }).collect::<Vec<_>>()
            ),
            Selector::Prefix { shard, sort_prefix } => (
                vec![shard.to_string()],
                vec![k2v_client::BatchReadOp {
                partition_key: shard,
                filter: k2v_client::Filter {
                    prefix: Some(sort_prefix),
                    ..k2v_client::Filter::default()
                },
                ..k2v_client::BatchReadOp::default()
            }]),
            Selector::Single(row_ref) => {
                let causal_value = match self.k2v.read_item(&row_ref.uid.shard, &row_ref.uid.sort).await {
                    Err(e) => {
                        tracing::error!("K2V read item shard={}, sort={}, bucket={} failed: {}", row_ref.uid.shard, row_ref.uid.sort, self.bucket, e);
                        return Err(StorageError::Internal);
                    },
                    Ok(v) => v,
                };

                let row_val = causal_to_row_val((*row_ref).clone(), causal_value);
                return Ok(vec![row_val])
            },
        };

        let all_raw_res = match self.k2v.read_batch(&batch_op).await {
            Err(e) => {
                tracing::error!("k2v read batch failed for {:?}, bucket {} with err: {}", select, self.bucket, e);
                return Err(StorageError::Internal);
            },
            Ok(v) => v,
        };

        let row_vals = all_raw_res
            .into_iter()
            .fold(vec![], |mut acc, v| {
                acc.extend(v.items);
                acc
            })
            .into_iter()
            .zip(pk_list.into_iter())
            .map(|((sk, cv), pk)| causal_to_row_val(RowRef::new(&pk, &sk), cv))
            .collect::<Vec<_>>(); 

        Ok(row_vals)
    }
    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        unimplemented!();
    }

    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        let batch_ops = values.iter().map(|v| k2v_client::BatchInsertOp {
            partition_key: &v.row_ref.uid.shard,
            sort_key: &v.row_ref.uid.sort,
            causality: v.row_ref.causality.clone().map(|ct| ct.into()),
            value: v.value.iter().next().map(|cv| match cv {
                Alternative::Value(buff) => k2v_client::K2vValue::Value(buff.clone()),
                Alternative::Tombstone => k2v_client::K2vValue::Tombstone,
            }).unwrap_or(k2v_client::K2vValue::Tombstone)
        }).collect::<Vec<_>>();

        match self.k2v.insert_batch(&batch_ops).await {
            Err(e) => {
                tracing::error!("k2v can't insert some value: {}", e);
                Err(StorageError::Internal)
            },
            Ok(v) => Ok(v),
        }
    }
    async fn row_poll(&self, value: &RowRef) -> Result<RowVal, StorageError> {
        loop {
            if let Some(ct) = &value.causality {
                match self.k2v.poll_item(&value.uid.shard, &value.uid.sort, ct.clone().into(), None).await {
                    Err(e) => {
                        tracing::error!("Unable to poll item: {}", e);
                        return Err(StorageError::Internal);
                    }
                    Ok(None) => continue,
                    Ok(Some(cv)) => return Ok(causal_to_row_val(value.clone(), cv)),
                }
            } else {
                match self.k2v.read_item(&value.uid.shard, &value.uid.sort).await {
                    Err(k2v_client::Error::NotFound) => {
                        self
                            .k2v
                            .insert_item(&value.uid.shard, &value.uid.sort, vec![0u8], None)
                            .await
                            .map_err(|e| {
                                tracing::error!("Unable to insert item in polling logic: {}", e);
                                StorageError::Internal
                            })?;
                    }
                    Err(e) => {
                        tracing::error!("Unable to read item in polling logic: {}", e);
                        return Err(StorageError::Internal)
                    },
                    Ok(cv) => return Ok(causal_to_row_val(value.clone(), cv)),
                }
            }
        }
    }

    async fn row_rm_single(&self, entry: &RowRef) -> Result<(), StorageError> {
        unimplemented!();
    }

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError> {
        let maybe_out =  self.s3
            .get_object()
            .bucket(self.bucket.to_string())
            .key(blob_ref.0.to_string())
            .send()
            .await;

        let object_output = match maybe_out {
            Ok(output) => output,
            Err(SdkError::ServiceError(x)) => match x.err() {
                GetObjectError::NoSuchKey(_) => return Err(StorageError::NotFound),
                e => {
                    tracing::warn!("Blob Fetch Error, Service Error: {}", e);
                    return Err(StorageError::Internal);
                },
            },
            Err(e) => {
                tracing::warn!("Blob Fetch Error, {}", e);
                return Err(StorageError::Internal);
            },
        };

        let buffer = match object_output.body.collect().await {
            Ok(aggreg) => aggreg.to_vec(),
            Err(e) => {
                tracing::warn!("Fetching body failed with {}", e);
                return Err(StorageError::Internal);
            }
        };

        tracing::debug!("Fetched {}/{}", self.bucket, blob_ref.0);
        Ok(BlobVal::new(blob_ref.clone(), buffer))
    }
    async fn blob_insert(&self, blob_val: BlobVal) -> Result<(), StorageError> {
        let streamable_value =  s3::primitives::ByteStream::from(blob_val.value);

        let maybe_send = self.s3
            .put_object()
            .bucket(self.bucket.to_string())
            .key(blob_val.blob_ref.0.to_string())
            .body(streamable_value)
            .send()
            .await;

        match maybe_send {
            Err(e) => {
                tracing::error!("unable to send object: {}", e);
                Err(StorageError::Internal)
            }
            Ok(_) => {
                tracing::debug!("Inserted {}/{}", self.bucket, blob_val.blob_ref.0);
                Ok(())
            }
        }
    }
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError> {
        let maybe_copy = self.s3
            .copy_object()
            .bucket(self.bucket.to_string())
            .key(dst.0.clone())
            .copy_source(format!("/{}/{}", self.bucket.to_string(), src.0.clone()))
            .send()
            .await;

        match maybe_copy {
            Err(e) => {
                tracing::error!("unable to copy object {} to {} (bucket: {}), error: {}", src.0, dst.0, self.bucket, e);
                Err(StorageError::Internal)
            },
            Ok(_) => {
                tracing::debug!("copied {} to {} (bucket: {})", src.0, dst.0, self.bucket);
                Ok(())
            }
        }

    }
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError> {
        let maybe_list = self.s3
            .list_objects_v2()
            .bucket(self.bucket.to_string())
            .prefix(prefix)
            .into_paginator()
            .send()
            .try_collect()
            .await;

        match maybe_list {
            Err(e) => {
                tracing::error!("listing prefix {} on bucket {} failed: {}", prefix, self.bucket, e);
                Err(StorageError::Internal)
            }
            Ok(pagin_list_out) => Ok(pagin_list_out
                .into_iter()
                .map(|list_out| list_out.contents.unwrap_or(vec![]))
                .flatten()
                .map(|obj| BlobRef(obj.key.unwrap_or(String::new())))
                .collect::<Vec<_>>()),
        }
    }
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError> {
        let maybe_delete = self.s3
            .delete_object()
            .bucket(self.bucket.to_string())
            .key(blob_ref.0.clone())
            .send()
            .await;

        match maybe_delete {
            Err(e) => {
                tracing::error!("unable to delete {} (bucket: {}), error {}", blob_ref.0, self.bucket, e);
                Err(StorageError::Internal)
            },
            Ok(_) => {
                tracing::debug!("deleted {} (bucket: {})", blob_ref.0, self.bucket);
                Ok(())
            }
        }
    }
}

