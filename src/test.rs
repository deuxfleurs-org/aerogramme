mod config; 

use serde::Serialize;
use std::collections::HashMap;

fn main() {
  let config = config::Config {
    s3_endpoint: "http://127.0.0.1:3900".to_string(),
    k2v_endpoint: "http://127.0.0.1:3904".to_string(),
    aws_region: "garage".to_string(),
    login_static: Some(config::LoginStaticConfig {
      default_bucket: Some("mailrage".to_string()),
      users: HashMap::from([
        ("quentin".to_string(), config::LoginStaticUser {
          password: "toto".to_string(),
          aws_access_key_id: "GKxxx".to_string(),
          aws_secret_access_key: "ffff".to_string(),
          bucket: Some("mailrage-quentin".to_string()),
          user_secret: "xxx".to_string(),
          alternate_user_secrets: vec![],
          master_key: None,
          secret_key: None,
        }),
      ]),
    }),
    login_ldap: None,
  };

  let ser = toml::to_string(&config).unwrap();
  println!("{}", ser);
}
