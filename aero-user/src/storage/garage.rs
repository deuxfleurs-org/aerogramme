use aws_sdk_s3::{self as s3, error::SdkError, operation::get_object::GetObjectError};
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use k2v_client::hyper_rustls::HttpsConnector;
use k2v_client::hyper_util::client::legacy::{connect::HttpConnector, Client as HttpClient};
use k2v_client::hyper_util::rt::TokioExecutor;
use serde::Serialize;

use super::*;

pub struct GarageRoot {
    k2v_http: HttpClient<HttpsConnector<HttpConnector>, k2v_client::Body>,
    aws_http: SharedHttpClient,
}

impl GarageRoot {
    pub fn new() -> anyhow::Result<Self> {
        let connector = k2v_client::hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let k2v_http = HttpClient::builder(TokioExecutor::new()).build(connector);
        let aws_http = HyperClientBuilder::new().build_https();
        Ok(Self { k2v_http, aws_http })
    }

    pub async fn user(&self, conf: GarageConf) -> anyhow::Result<Arc<GarageUser>> {
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.append(&mut rmp_serde::to_vec(&conf)?);

        let s3_creds = s3::config::Credentials::new(
            conf.aws_access_key_id.clone(),
            conf.aws_secret_access_key.clone(),
            None,
            None,
            "aerogramme",
        );

        let sdk_config = aws_config::from_env()
            .region(aws_config::Region::new(conf.region.clone()))
            .credentials_provider(s3_creds)
            .http_client(self.aws_http.clone())
            .endpoint_url(conf.s3_endpoint.clone())
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

        let k2v_config = k2v_client::K2vClientConfig {
            endpoint: conf.k2v_endpoint.clone(),
            region: conf.region.clone(),
            aws_access_key_id: conf.aws_access_key_id.clone(),
            aws_secret_access_key: conf.aws_secret_access_key.clone(),
            bucket: conf.bucket.clone(),
            user_agent: None,
        };

        let k2v_client =
            match k2v_client::K2vClient::new_with_client(k2v_config, self.k2v_http.clone()) {
                Err(e) => {
                    anyhow::bail!("unable to build k2v client: {}", e)
                }
                Ok(v) => v,
            };

        Ok(Arc::new(GarageUser {
            conf,
            s3: s3_client,
            k2v: k2v_client,
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

#[derive(Debug)]
pub struct GarageUser {
    conf: GarageConf,
    s3: s3::Client,
    k2v: k2v_client::K2vClient,
    unicity: Vec<u8>,
}

#[async_trait]
impl IBuilder for GarageUser {
    async fn build(&self) -> Result<Store, StorageError> {
        Ok(Box::new(GarageStore {
            bucket: self.conf.bucket.clone(),
            s3: self.s3.clone(),
            k2v: self.k2v.clone(),
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

fn causal_to_concurrent_row_val(shard: &str, sort: &str, causal_value: k2v_client::CausalValue) -> ConcurrentRowVal {
    let new_row_ref = RowRef::new(shard, sort).with_causality(causal_value.causality.into());
    let row_values = causal_value
        .value
        .into_iter()
        .map(|k2v_value| match k2v_value {
            k2v_client::K2vValue::Tombstone => Alternative::Tombstone,
            k2v_client::K2vValue::Value(v) => Alternative::Value(v),
        })
        .collect::<Vec<_>>();

    ConcurrentRowVal {
        row_ref: new_row_ref,
        value: row_values,
    }
}

#[async_trait]
impl IStore for GarageStore {
    async fn row_fetch(&self, shard: &str, sort: &str) -> Result<ConcurrentRowVal, StorageError> {
        tracing::trace!(shard=%shard, sort=%sort, command="row_fetch");
        let causal_value = match self
            .k2v
            .read_item(shard, sort)
            .await
        {
            Err(k2v_client::Error::NotFound) => {
                tracing::debug!(
                    "K2V item not found  shard={}, sort={}, bucket={}",
                    shard,
                    sort,
                    self.bucket,
                );
                return Err(StorageError::NotFound);
            }
            Err(e) => {
                tracing::error!(
                    "K2V read item shard={}, sort={}, bucket={} failed: {}",
                    shard,
                    sort,
                    self.bucket,
                    e
                );
                return Err(StorageError::Internal);
            }
            Ok(v) => v,
        };

        Ok(causal_to_concurrent_row_val(shard, sort, causal_value))
    }
    
    async fn row_fetch_batch<'a>(&self, select: &Selector<'a>) -> Result<Vec<ConcurrentRowVal>, StorageError> {
        tracing::trace!(select=%select, command="row_fetch_batch");
        let (shard, batch_op) = match select {
            Selector::Range {
                shard,
                sort_begin,
                sort_end,
            } => (
                shard,
                vec![k2v_client::BatchReadOp {
                    partition_key: shard,
                    filter: k2v_client::Filter {
                        start: *sort_begin,
                        end: *sort_end,
                        ..k2v_client::Filter::default()
                    },
                    ..k2v_client::BatchReadOp::default()
                }],
            ),
            Selector::List { shard, sort_list } => (
                shard,
                sort_list
                    .iter()
                    .map(|sort| k2v_client::BatchReadOp {
                        partition_key: shard,
                        filter: k2v_client::Filter {
                            start: Some(sort),
                            ..k2v_client::Filter::default()
                        },
                        single_item: true,
                        ..k2v_client::BatchReadOp::default()
                    })
                    .collect::<Vec<_>>(),
            ),
            Selector::Prefix { shard, sort_prefix } => (
                shard,
                vec![k2v_client::BatchReadOp {
                    partition_key: shard,
                    filter: k2v_client::Filter {
                        prefix: Some(sort_prefix),
                        ..k2v_client::Filter::default()
                    },
                    ..k2v_client::BatchReadOp::default()
                }],
            ),
            Selector::Single { shard, sort } => (
                shard,
                vec![k2v_client::BatchReadOp {
                    partition_key: shard,
                    filter: k2v_client::Filter {
                        start: Some(sort),
                        ..k2v_client::Filter::default()
                    },
                    single_item: true,
                    ..k2v_client::BatchReadOp::default()
                }]
            )
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
                .fold(vec![], |mut acc, page| {
                    page.items
                        .into_iter()
                        .map(|(sk, cv)| causal_to_concurrent_row_val(shard, &sk, cv))
                        .for_each(|rr| acc.push(rr));

                    acc
                });
        tracing::debug!(fetch_count = row_vals.len(), command = "row_fetch");

        Ok(row_vals)
    }
    async fn row_delete_batch<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        tracing::trace!(select=%select, command="row_delete_batch");
        let del_op = match select {
            Selector::Range {
                shard,
                sort_begin,
                sort_end,
            } => vec![k2v_client::BatchDeleteOp {
                partition_key: shard,
                prefix: None,
                start: *sort_begin,
                end: *sort_end,
                single_item: false,
            }],
            Selector::List { shard, sort_list } =>
                sort_list
                    .iter()
                    .map(|sort| k2v_client::BatchDeleteOp {
                        partition_key: shard,
                        prefix: None,
                        start: Some(sort),
                        end: None,
                        single_item: true,
                    })
                    .collect::<Vec<_>>(),
            Selector::Prefix { shard, sort_prefix } => vec![k2v_client::BatchDeleteOp {
                partition_key: shard,
                prefix: Some(sort_prefix),
                start: None,
                end: None,
                single_item: false,
            }],
            Selector::Single { shard, sort } => vec![k2v_client::BatchDeleteOp {
                partition_key: shard,
                prefix: None,
                start: Some(sort),
                end: None,
                single_item: true,
            }],
        };

        match self.k2v.delete_batch(&del_op).await {
            Err(e) => {
                tracing::error!("delete batch error: {}", e);
                Err(StorageError::Internal)
            }
            Ok(_) => Ok(()),
        }
    }

    async fn row_update(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        tracing::trace!(entries=%values.iter().map(|v| v.row_ref.to_string()).collect::<Vec<_>>().join(","), command="row_update");
        let batch_ops = values
            .iter()
            .map(|v| k2v_client::BatchInsertOp {
                partition_key: &v.row_ref.uid.shard,
                sort_key: &v.row_ref.uid.sort,
                causality: v.row_ref.causality.clone().map(|ct| ct.into()),
                value: match &v.value {
                    Alternative::Value(buff) => k2v_client::K2vValue::Value(buff.clone()),
                    Alternative::Tombstone => k2v_client::K2vValue::Tombstone,
                },
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
    async fn row_poll(&self, value: &RowRef) -> Result<ConcurrentRowVal, StorageError> {
        tracing::trace!(entry=%value, command="row_poll");
        // the k2v poll periodically timeouts when nothing happens;
        // we automatically retry when it does.
        loop {
            let shard = &value.uid.shard;
            let sort = &value.uid.sort;
            if let Some(ct) = &value.causality {
                match self
                    .k2v
                    .poll_item(shard, sort, ct.clone().into(), None)
                    .await
                {
                    Err(e) => {
                        tracing::error!("Unable to poll item: {}", e);
                        return Err(StorageError::Internal);
                    }
                    Ok(None) => continue,
                    Ok(Some(cv)) => return Ok(causal_to_concurrent_row_val(shard, sort, cv)),
                }
            } else {
                // `row_poll` must support polling without causality
                // information. However, K2V PollItem requires that we pass a
                // causality token. If we don't have one, we do a read instead
                // and return immediately.
                match self.k2v.read_item(shard, sort).await {
                    Err(k2v_client::Error::NotFound) => {
                        return Err(StorageError::NotFound)
                    }
                    Err(e) => {
                        tracing::error!("Unable to read item in polling logic: {}", e);
                        return Err(StorageError::Internal);
                    }
                    Ok(cv) => return Ok(causal_to_concurrent_row_val(shard, sort, cv)),
                }
            }
        }
    }
    async fn row_poll_range<'a>(&self, select: &RangeSelector<'a>, seen_marker: Option<&str>) ->
        Result<PollRangeResult, StorageError>
    {
        tracing::trace!(select=%select, command="row_poll_range");
        // the k2v poll periodically timeouts when nothing happens; we automatically retry
        loop {
            let (shard, filter) = match select {
                RangeSelector::Range { shard, sort_begin, sort_end } => (
                    shard,
                    k2v_client::PollRangeFilter {
                        start: *sort_begin,
                        end: *sort_end,
                        prefix: None,
                    }
                ),
                RangeSelector::Prefix { shard, sort_prefix } => (
                    shard,
                    k2v_client::PollRangeFilter {
                        start: None,
                        end: None,
                        prefix: Some(*sort_prefix),
                    }
                )
            };

            match self.k2v.poll_range(shard, Some(filter), seen_marker, None).await {
                Err(e) => {
                    tracing::error!("Unable to poll range: {}", e);
                    return Err(StorageError::Internal);
                }
                Ok(None) => continue,
                Ok(Some(res)) => {
                    let value = res
                        .items
                        .into_iter()
                        .map(|(sort, cv)| causal_to_concurrent_row_val(shard, &sort, cv))
                        .collect();
                    return Ok(PollRangeResult { value, seen_marker: res.seen_marker })
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
