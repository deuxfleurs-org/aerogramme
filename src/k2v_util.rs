use anyhow::Result;

use k2v_client::{CausalValue, CausalityToken, K2vClient};

// ---- UTIL: function to wait for a value to have changed in K2V ----

pub async fn k2v_wait_value_changed(
    k2v: &K2vClient,
    pk: &str,
    sk: &str,
    prev_ct: &Option<CausalityToken>,
) -> Result<CausalValue> {
    loop {
        if let Some(ct) = prev_ct {
            match k2v.poll_item(pk, sk, ct.clone(), None).await? {
                None => continue,
                Some(cv) => return Ok(cv),
            }
        } else {
            match k2v.read_item(pk, sk).await {
                Err(k2v_client::Error::NotFound) => {
                    k2v.insert_item(pk, sk, vec![0u8], None).await?;
                }
                Err(e) => return Err(e.into()),
                Ok(cv) => return Ok(cv),
            }
        }
    }
}
