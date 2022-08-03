# Setup

You must start by creating a user profile in Garage. Run the following command after adjusting the parameters to your configuration:

```bash
cargo run -- first-login \
  --region garage \
  --k2v-endpoint http://127.0.0.1:3904 \
  --s3-endpoint http://127.0.0.1:3900 \
  --aws-access-key-id GK... \
  --aws-secret-access-key c0ffee... \
  --bucket mailrage-me \
  --user-secret s3cr3t
```

*Note: user-secret is not the user's password. It is an additional secret used when deriving user's secret key from their password. The idea is that, even if user leaks their password, their encrypted data remain safe as long as this additional secret does not leak. You can generate it with openssl for example: `openssl rand -base64 30`. Read [Cryptography & key management](./crypt-key.md) for more details.*


The program will interactively ask you some questions and finally generates for you a snippet of configuration:

```
Please enter your password for key decryption.
If you are using LDAP login, this must be your LDAP password.
If you are using the static login provider, enter any password, and this will also become your password for local IMAP access.
Enter password:
Confirm password:

Cryptographic key setup is complete.

If you are using the static login provider, add the following section to your .toml configuration file:

[login_static.users.<username>]
password = "$argon2id$v=19$m=4096,t=3,p=1$..."
aws_access_key_id = "GK..."
aws_secret_access_key = "c0ffee..."
```

In this tutorial, we will use the static login provider (and not the LDAP one).
We will thus create a config file named `aerogramme.toml` in which we will paste the previous snippet. You also need to enter some other keys. In the end, your file should look like that:

```toml
s3_endpoint = "http://127.0.0.1:3900"
k2v_endpoint = "http://127.0.0.1:3904"
aws_region = "garage"

[lmtp]
bind_addr = "[::1]:12024"
hostname = "aerogramme.tld"

[imap]
bind_addr = "[::1]:1993"

[login_static]
default_bucket = "mailrage"

[login_static.users.me]
bucket = "mailrage-me"
user_secret = "s3cr3t"
email_addresses = [
  "me@aerogramme.tld"
]

# copy pasted values from first-login
password = "$argon2id$v=19$m=4096,t=3,p=1$..."
aws_access_key_id = "GK..."
aws_secret_access_key = "c0ffee..."
```

If you fear to loose your password, you can backup your key with the following command:

```bash
cargo run -- show-keys \
  --region garage \
  --k2v-endpoint http://127.0.0.1:3904 \
  --s3-endpoint http://127.0.0.1:3900 \
  --aws-access-key-id GK... \
  --aws-secret-access-key c0ffee... \
  --bucket mailrage-me \
  --user-secret s3cr3t
```

You will then be asked for your key decryption password:

```
Enter key decryption password:
master_key = "..."
secret_key = "..."
```

You are now ready to [validate your installation](./validate.md).
