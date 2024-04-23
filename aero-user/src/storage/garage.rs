use aws_sdk_s3::{self as s3, error::SdkError, operation::get_object::GetObjectError};
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::{connect::HttpConnector, Client as HttpClient};
use hyper_util::rt::TokioExecutor;
use serde::Serialize;

use super::*;

pub struct GarageRoot {
    k2v_http: HttpClient<HttpsConnector<HttpConnector>, k2v_client::Body>,
    aws_http: SharedHttpClient,
}

impl GarageRoot {
    pub fn new() -> anyhow::Result<Self> {
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let k2v_http = HttpClient::builder(TokioExecutor::new()).build(connector);
        let aws_http = HyperClientBuilder::new().build_https();
        Ok(Self { k2v_http, aws_http })
    }

    pub fn user(&self, conf: GarageConf) -> anyhow::Result<Arc<GarageUser>> {
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.append(&mut rmp_serde::to_vec(&conf)?);

        Ok(Arc::new(GarageUser {
            conf,
            aws_http: self.aws_http.clone(),
            k2v_http: self.k2v_http.clone(),
            unicity,
        }))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct GarageConf {
    pub region: String,
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

//@FIXME we should get rid of this builder
//and allocate a S3 + K2V client only once per user
//(and using a shared HTTP client)
#[derive(Clone, Debug)]
pub struct GarageUser {
    conf: GarageConf,
    aws_http: SharedHttpClient,
    k2v_http: HttpClient<HttpsConnector<HttpConnector>, k2v_client::Body>,
    unicity: Vec<u8>,
}

#[async_trait]
impl IBuilder for GarageUser {
    async fn build(&self) -> Result<Store, StorageError> {
        let s3_creds = s3::config::Credentials::new(
            self.conf.aws_access_key_id.clone(),
            self.conf.aws_secret_access_key.clone(),
            None,
            None,
            "aerogramme",
        );

        let sdk_config = aws_config::from_env()
            .region(aws_config::Region::new(self.conf.region.clone()))
            .credentials_provider(s3_creds)
            .http_client(self.aws_http.clone())
            .endpoint_url(self.conf.s3_endpoint.clone())
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

        let k2v_config = k2v_client::K2vClientConfig {
            endpoint: self.conf.k2v_endpoint.clone(),
            region: self.conf.region.clone(),
            aws_access_key_id: self.conf.aws_access_key_id.clone(),
            aws_secret_access_key: self.conf.aws_secret_access_key.clone(),
            bucket: self.conf.bucket.clone(),
            user_agent: None,
        };

        let k2v_client =
            match k2v_client::K2vClient::new_with_client(k2v_config, self.k2v_http.clone()) {
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
    let row_values = causal_value
        .value
        .into_iter()
        .map(|k2v_value| match k2v_value {
            k2v_client::K2vValue::Tombstone => Alternative::Tombstone,
            k2v_client::K2vValue::Value(v) => Alternative::Value(v),
        })
        .collect::<Vec<_>>();

    RowVal {
        row_ref: new_row_ref,
        value: row_values,
    }
}

#[async_trait]
impl IStore for GarageStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError> {
        tracing::trace!(select=%select, command="row_fetch");
        let (pk_list, batch_op) = match select {
            Selector::Range {
                shard,
                sort_begin,
                sort_end,
            } => (
                vec![shard.to_string()],
                vec![k2v_client::BatchReadOp {
                    partition_key: shard,
                    filter: k2v_client::Filter {
                        start: Some(sort_begin),
                        end: Some(sort_end),
                        ..k2v_client::Filter::default()
                    },
                    ..k2v_client::BatchReadOp::default()
                }],
            ),
            Selector::List(row_ref_list) => (
                row_ref_list
                    .iter()
                    .map(|row_ref| row_ref.uid.shard.to_string())
                    .collect::<Vec<_>>(),
                row_ref_list
                    .iter()
                    .map(|row_ref| k2v_client::BatchReadOp {
                        partition_key: &row_ref.uid.shard,
                        filter: k2v_client::Filter {
                            start: Some(&row_ref.uid.sort),
                            ..k2v_client::Filter::default()
                        },
                        single_item: true,
                        ..k2v_client::BatchReadOp::default()
                    })
                    .collect::<Vec<_>>(),
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
                }],
            ),
            Selector::Single(row_ref) => {
                let causal_value = match self
                    .k2v
                    .read_item(&row_ref.uid.shard, &row_ref.uid.sort)
                    .await
                {
                    Err(k2v_client::Error::NotFound) => {
                        tracing::debug!(
                            "K2V item not found  shard={}, sort={}, bucket={}",
                            row_ref.uid.shard,
                            row_ref.uid.sort,
                            self.bucket,
                        );
                        return Err(StorageError::NotFound);
                    }
                    Err(e) => {
                        tracing::error!(
                            "K2V read item shard={}, sort={}, bucket={} failed: {}",
                            row_ref.uid.shard,
                            row_ref.uid.sort,
                            self.bucket,
                            e
                        );
                        return Err(StorageError::Internal);
                    }
                    Ok(v) => v,
                };

                let row_val = causal_to_row_val((*row_ref).clone(), causal_value);
                return Ok(vec![row_val]);
            }
        };

        let all_raw_res = match self.k2v.read_batch(&batch_op).await {
            Err(e) => {
                tracing::error!(
                    "k2v read batch failed for {:?}, bucket {} with err: {}",
                    select,
                    self.bucket,
                    e
                );
                return Err(StorageError::Internal);
            }
            Ok(v) => v,
        };
        //println!("fetch res -> {:?}", all_raw_res);

        let row_vals =
            all_raw_res
                .into_iter()
                .zip(pk_list.into_iter())
                .fold(vec![], |mut acc, (page, pk)| {
                    page.items
                        .into_iter()
                        .map(|(sk, cv)| causal_to_row_val(RowRef::new(&pk, &sk), cv))
                        .for_each(|rr| acc.push(rr));

                    acc
                });
        tracing::debug!(fetch_count = row_vals.len(), command = "row_fetch");

        Ok(row_vals)
    }
    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        tracing::trace!(select=%select, command="row_rm");
        let del_op = match select {
            Selector::Range {
                shard,
                sort_begin,
                sort_end,
            } => vec![k2v_client::BatchDeleteOp {
                partition_key: shard,
                prefix: None,
                start: Some(sort_begin),
                end: Some(sort_end),
                single_item: false,
            }],
            Selector::List(row_ref_list) => {
                // Insert null values with causality token = delete
                let batch_op = row_ref_list
                    .iter()
                    .map(|v| k2v_client::BatchInsertOp {
                        partition_key: &v.uid.shard,
                        sort_key: &v.uid.sort,
                        causality: v.causality.clone().map(|ct| ct.into()),
                        value: k2v_client::K2vValue::Tombstone,
                    })
                    .collect::<Vec<_>>();

                return match self.k2v.insert_batch(&batch_op).await {
                    Err(e) => {
                        tracing::error!("Unable to delete the list of values: {}", e);
                        Err(StorageError::Internal)
                    }
                    Ok(_) => Ok(()),
                };
            }
            Selector::Prefix { shard, sort_prefix } => vec![k2v_client::BatchDeleteOp {
                partition_key: shard,
                prefix: Some(sort_prefix),
                start: None,
                end: None,
                single_item: false,
            }],
            Selector::Single(row_ref) => {
                // Insert null values with causality token = delete
                let batch_op = vec![k2v_client::BatchInsertOp {
                    partition_key: &row_ref.uid.shard,
                    sort_key: &row_ref.uid.sort,
                    causality: row_ref.causality.clone().map(|ct| ct.into()),
                    value: k2v_client::K2vValue::Tombstone,
                }];

                return match self.k2v.insert_batch(&batch_op).await {
                    Err(e) => {
                        tracing::error!("Unable to delete the list of values: {}", e);
                        Err(StorageError::Internal)
                    }
                    Ok(_) => Ok(()),
                };
            }
        };

        // Finally here we only have prefix & range
        match self.k2v.delete_batch(&del_op).await {
            Err(e) => {
                tracing::error!("delete batch error: {}", e);
                Err(StorageError::Internal)
            }
            Ok(_) => Ok(()),
        }
    }

    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        tracing::trace!(entries=%values.iter().map(|v| v.row_ref.to_string()).collect::<Vec<_>>().join(","), command="row_insert");
        let batch_ops = values
            .iter()
            .map(|v| k2v_client::BatchInsertOp {
                partition_key: &v.row_ref.uid.shard,
                sort_key: &v.row_ref.uid.sort,
                causality: v.row_ref.causality.clone().map(|ct| ct.into()),
                value: v
                    .value
                    .iter()
                    .next()
                    .map(|cv| match cv {
                        Alternative::Value(buff) => k2v_client::K2vValue::Value(buff.clone()),
                        Alternative::Tombstone => k2v_client::K2vValue::Tombstone,
                    })
                    .unwrap_or(k2v_client::K2vValue::Tombstone),
            })
            .collect::<Vec<_>>();

        match self.k2v.insert_batch(&batch_ops).await {
            Err(e) => {
                tracing::error!("k2v can't insert some value: {}", e);
                Err(StorageError::Internal)
            }
            Ok(v) => Ok(v),
        }
    }
    async fn row_poll(&self, value: &RowRef) -> Result<RowVal, StorageError> {
        tracing::trace!(entry=%value, command="row_poll");
        loop {
            if let Some(ct) = &value.causality {
                match self
                    .k2v
                    .poll_item(&value.uid.shard, &value.uid.sort, ct.clone().into(), None)
                    .await
                {
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
                        self.k2v
                            .insert_item(&value.uid.shard, &value.uid.sort, vec![0u8], None)
                            .await
                            .map_err(|e| {
                                tracing::error!("Unable to insert item in polling logic: {}", e);
                                StorageError::Internal
                            })?;
                    }
                    Err(e) => {
                        tracing::error!("Unable to read item in polling logic: {}", e);
                        return Err(StorageError::Internal);
                    }
                    Ok(cv) => return Ok(causal_to_row_val(value.clone(), cv)),
                }
            }
        }
    }

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError> {
        tracing::trace!(entry=%blob_ref, command="blob_fetch");
        let maybe_out = self
            .s3
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
                }
            },
            Err(e) => {
                tracing::warn!("Blob Fetch Error, {}", e);
                return Err(StorageError::Internal);
            }
        };

        let buffer = match object_output.body.collect().await {
            Ok(aggreg) => aggreg.to_vec(),
            Err(e) => {
                tracing::warn!("Fetching body failed with {}", e);
                return Err(StorageError::Internal);
            }
        };

        let mut bv = BlobVal::new(blob_ref.clone(), buffer);
        if let Some(meta) = object_output.metadata {
            bv.meta = meta;
        }
        tracing::debug!("Fetched {}/{}", self.bucket, blob_ref.0);
        Ok(bv)
    }
    async fn blob_insert(&self, blob_val: BlobVal) -> Result<String, StorageError> {
        tracing::trace!(entry=%blob_val.blob_ref, command="blob_insert");
        let streamable_value = s3::primitives::ByteStream::from(blob_val.value);
        let obj_key = blob_val.blob_ref.0;

        let maybe_send = self
            .s3
            .put_object()
            .bucket(self.bucket.to_string())
            .key(obj_key.to_string())
            .set_metadata(Some(blob_val.meta))
            .body(streamable_value)
            .send()
            .await;

        match maybe_send {
            Err(e) => {
                tracing::error!("unable to send object: {}", e);
                Err(StorageError::Internal)
            }
            Ok(put_output) => {
                tracing::debug!("Inserted {}/{}", self.bucket, obj_key);
                Ok(put_output
                    .e_tag()
                    .map(|v| format!("\"{}\"", v))
                    .unwrap_or(format!("W/\"{}\"", obj_key)))
            }
        }
    }
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError> {
        tracing::trace!(src=%src, dst=%dst, command="blob_copy");
        let maybe_copy = self
            .s3
            .copy_object()
            .bucket(self.bucket.to_string())
            .key(dst.0.clone())
            .copy_source(format!("/{}/{}", self.bucket.to_string(), src.0.clone()))
            .send()
            .await;

        match maybe_copy {
            Err(e) => {
                tracing::error!(
                    "unable to copy object {} to {} (bucket: {}), error: {}",
                    src.0,
                    dst.0,
                    self.bucket,
                    e
                );
                Err(StorageError::Internal)
            }
            Ok(_) => {
                tracing::debug!("copied {} to {} (bucket: {})", src.0, dst.0, self.bucket);
                Ok(())
            }
        }
    }
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError> {
        tracing::trace!(prefix = prefix, command = "blob_list");
        let maybe_list = self
            .s3
            .list_objects_v2()
            .bucket(self.bucket.to_string())
            .prefix(prefix)
            .into_paginator()
            .send()
            .try_collect()
            .await;

        match maybe_list {
            Err(e) => {
                tracing::error!(
                    "listing prefix {} on bucket {} failed: {}",
                    prefix,
                    self.bucket,
                    e
                );
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
        tracing::trace!(entry=%blob_ref, command="blob_rm");
        let maybe_delete = self
            .s3
            .delete_object()
            .bucket(self.bucket.to_string())
            .key(blob_ref.0.clone())
            .send()
            .await;

        match maybe_delete {
            Err(e) => {
                tracing::error!(
                    "unable to delete {} (bucket: {}), error {}",
                    blob_ref.0,
                    self.bucket,
                    e
                );
                Err(StorageError::Internal)
            }
            Ok(_) => {
                tracing::debug!("deleted {} (bucket: {})", blob_ref.0, self.bucket);
                Ok(())
            }
        }
    }
}
