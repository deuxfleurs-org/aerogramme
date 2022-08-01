# Configuration file

A configuration file that illustrate all the possible options,
in practise, many fields are omitted:

```toml
s3_endpoint = "s3.garage.tld"
k2v_endpoint = "k2v.garage.tld"
aws_region = "garage"

[lmtp]
bind_addr = "[::1]:2525"
hostname = "aerogramme.tld"

[imap]
bind_addr = "[::1]:993"

[login_static]
default_bucket = "aerogramme"

[login_static.user.alan]
email_addresses = [
  "alan@smith.me"
  "aln@example.com"
]
password = "$argon2id$v=19$m=4096,t=3,p=1$..."

aws_access_key_id = "GK..."
aws_secret_access_key = "c0ffee"
bucket = "aerogramme-alan"

user_secret = "s3cr3t"
alternate_user_secrets = [ "s3cr3t2" "s3cr3t3" ]

master_key = "..."
secret_key = "..."

[login_ldap]
ldap_server = "ldap.example.com"

pre_bind_on_login = true
bind_dn = "cn=admin,dc=example,dc=com"
bind_password = "s3cr3t"

search_base = "ou=users,dc=example,dc=com"
username_attr = "cn"
mail_attr = "mail"

aws_access_key_id_attr = "garage_s3_access_key"
aws_secret_access_key_attr = "garage_s3_secret_key"
user_secret_attr = "secret"
alternate_user_secrets_attr = "secret_alt"

# bucket = "aerogramme"
bucket_attr = "bucket"

```

## Global configuration options

### `s3_endpoint`

### `k2v_endpoint`

### `aws_region`

## LMTP configuration options

### `lmtp.bind_addr`

### `lmtp.hostname`

## IMAP configuration options

### `imap.bind_addr`

## Static login configuration options

### `login_static.default_bucket`

### `login_static.user.<name>.email_addresses`

### `login_static.user.<name>.password`

### `login_static.user.<name>.aws_access_key_id`

### `login_static.user.<name>.aws_secret_access_key`

### `login_static.user.<name>.bucket`

### `login_static.user.<name>.user_secret`

### `login_static.user.<name>.master_key`

### `login_static.user.<name>.secret_key`

## LDAP login configuration options

### `login_ldap.ldap_server`

### `login_ldap.pre_bind_on`

### `login_ldap.bind_dn`

### `login_ldap.bind_password`

### `login_ldap.search_base`

### `login_ldap.username_attr`

### `login_ldap.mail_attr`

### `login_ldap.aws_access_key_id_attr`

### `login_ldap.aws_secret_access_key_attr`

### `login_ldap.user_secret_attr`

### `login_ldap.alternate_user_secrets_attr`

### `login_ldap.bucket`

### `login_ldap.bucket_attr`



