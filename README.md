# Aerogramme - Encrypted e-mail storage over Garage

## Usage

Start by running:

```
$ cargo run --bin main -- first-login --region garage --k2v-endpoint http://127.0.0.1:3904 --s3-endpoint http://127.0.0.1:3900 --aws-access-key-id GK... --aws-secret-access-key c0ffee... --bucket mailrage-quentin --user-secret poupou
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

Next create the config file `aerogramme.toml`:

```
s3_endpoint = "http://127.0.0.1:3900"
k2v_endpoint = "http://127.0.0.1:3904"
aws_region = "garage"

[login_static]
default_bucket = "mailrage"
[login_static.users.quentin]
bucket = "mailrage-quentin"
user_secret = "poupou"
alternate_user_secrets = []
password = "$argon2id$v=19$m=4096,t=3,p=1$..."
aws_access_key_id = "GK..."
aws_secret_access_key = "c0ffee..."
```

You can dump your keys with:

```
$ cargo run --bin main -- show-keys --region garage --k2v-endpoint http://127.0.0.1:3904 --s3-endpoint http://127.0.0.1:3900 --aws-access-key-id GK... --aws-secret-access-key c0ffee... --bucket mailrage-quentin --user-secret poupou
Enter key decryption password:
master_key = "..."
secret_key = "..."
```

Run a test instance with:

```
$ cargo run --bin main -- server
---- MAILBOX STATE ----
UIDVALIDITY 1
UIDNEXT 2
INTERNALSEQ 2
1 c3d4524f557f19108480063f3216afa20000000000000000 \Unseen

---- MAILBOX STATE ----
UIDVALIDITY 1
UIDNEXT 3
INTERNALSEQ 3
1 c3d4524f557f19108480063f3216afa20000000000000000 \Unseen
2 6a1ab4d87af3d424a3a8f8720c4db3b60000000000000000 \Unseen
```


## Bayou storage module

Checkpoints are stored in S3 at `<path>/checkpoint/<timestamp>`. Example:

```
348 TestMailbox/checkpoint/00000180d77400dc126b16aac546b769
369 TestMailbox/checkpoint/00000180d776e509b68fdc5c376d0abc
357 TestMailbox/checkpoint/00000180d77a7fe68f4f76e3b45aa751
```

Operations are stored in K2V at PK `<path>`, SK `<timestamp>`. Example:

```
TestMailbox 00000180d77400dc126b16aac546b769 RcIsESv7WrjMuHwyI/dvCnkIfy6op5Tiylf0WSnn94aMS2uagl7YeMBwdv09TiSXBpu5nJ5e/9QFSfuEI/NqKrdQkX54MOsnaIGhRb0oqUG3KNaar3BiVSvYvXuzYhk4ii+TUS2Eyd6fCCaNVNM5
TestMailbox 00000180d775f27f5542a13fc21c665e RrTSOup/zO1Ei+QrjBcDLt4vvFSY+WJPBodwY64wy2ftW+Oh3VSArvlO4SAEPmdsx1gt0HPBZYR/OkVWsZpmix1ZLFUmvdib+rjNkorHQW1p+oLVK8tolGrqk4SRwl88cqu466T4vBEpDu7tRbH0
TestMailbox 00000180d775f292b3c8da00718389b4 VAwd8SRycIwsipZW5AcSG+EIYZVWn/Uj/TADbWhb4x5LVMceiRBHWVquY08RgT/lJKdhIcUqBA15bVG3klIg8tLsWJVG784NbsZwdGRczWmngcA=
TestMailbox 00000180d775f29d24842cf375d679e0 /FbXtEwm/bijtvOdqM1XFvKUalQFAOPHp+vF9jZThZn/viY5a6W1PyHeI8kTusF6EsVPAwPHpQyjIv/ghskC0f+zUEsSUhDwQANdwLNqDLAvTA==
TestMailbox 00000180d7768ab1dc01ff504e887c62 W/fF0WitpxJ05yHeOv96BlpGymT1kVOjkIW00t9e6UE7mxkvNflu9cZSCd8PDJd2ymC0sC9bLVFAXKmNZsmCFEEHMQSyrX61qTYo4KFCZMp5zm6fXubaYuurrzjXzfUP/R7kBvICFZlF0daf0SwX
TestMailbox 00000180d7768aba629c7ad6adf25228 IPzYGNsSepCX2AEnee/1Eas9a3c5esPSmrNkvaj4XcFb6Ft2KC8N6ubUR3wB+K0oYCTQym6nhHG5dlAxf6NRu7Rk8YtBTBmSqtGqd6kMZ3bU5b8=
TestMailbox 00000180d7768ac1870cda61784114d4 aaLiaWxfx1mxh6aoKE3xUUfZWhivZ/K7ixabflFDW7FO/qbpvCaa+Y6w4lQemTy6m+leAhXGN+Dbyv2qP20yJ9O4oJF5d3Lz5Iv5uF18OxhVZzw=
TestMailbox 00000180d776e4fb294ccdab2612b406 EtUPrLgEeOyab2QRnSie4I3Me9dDh10UdwWnUKdGa/8ezMJDtiy7XlW+tUfJdqtu6Vj7nduT0emDOXbBZsNwlcmzgYNwuNu3I9AfhZTFWtwLgB+wnAgB/jim82DDrJfLia8kB2eA2ao5jfJ3uMSZ
TestMailbox 00000180d776e501528546d340490291 Lz4Z9wCTk1lZ86lL01urhAan4oHcr1NBqdRe+CDpA51D9IncA5+Fhc8I6knUIh2qQ5/woWgISLAVwzSS+0+TxrYoqxf5FumIQtUJfwDER5La3n0=
TestMailbox 00000180d776e509b68fdc5c376d0abc RUGE2xB3fFX/wRH/p2fHIUa+rMaXSRd7fY9zglw0pRfVPqJfpniOjAe4GHIwGlwbwjtFOwS5a+Q7yr0Wez6QwD+ohhqRFKpbjcFcN7VfMyVAf+k=
TestMailbox 00000180d7784b987a8ad8106dc400c9 K+0LVEtBbTnWNS67jy9DtTvQyd5arovduvu490tLOE2TzVhuVoF4pfvTMTN12bH3KwEAHeDfuwKkKJFqldOywouTYPzEjZFkJzyagHrkl6dfnE5CqmlDv+Vc5TOQRskxjW+wQiZdjU8wGiBiBGYh
TestMailbox 00000180d7784bede69ac3cff2c6b724 XMFY3+b1r1//uolVz80JSI3g/84XCk3Tm7/S0BFv+Qe/Xv3/poLrOvAKEe+GzD2s22j8p/T2RXR/JSZckzgjEZeO0wbPDXVQd94di2Pff7jxAH8=
TestMailbox 00000180d7784bffe2595abe7ed81858 QQZhF+7wSHfikoAp93a+UY/XDIX7TVnnVYOtmQ2XHnDKA2F6snRJCPbYBO4IRHCRfVrjDGi32c41it2C3Mu5PBepabxapsW1rfIV3rlX2lkKHtI=
TestMailbox 00000180d77a7fb3f01dbb147c20cf7f IHOlOa1JI11RUKVvQUq3HQPxiRr4UCeE+pHmL8DtNMkOh62V4spuP0VvvQTJCQcPQ1EQR/QcxZ3s7uHLkrZAHF30BkpUkGqsLBWpnyug/puhdiixWsMyLLb6G90zFjiComUwptnDc/CCXtGEHdSW
TestMailbox 00000180d77a7fbb54b100f521ceb347 Ze4KyyTCgrYbZlXlJSY5hNob8sMXvBAmwIx2cADbX5P0M1IHXwXfloEzvvd6WYOtatFC2GnDSrmQ6RdCfeZ3WV9TZilqa0Fv0XEg48sVyVCcguw=
TestMailbox 00000180d77a7fe68f4f76e3b45aa751 cJJVvvRzTVNKUaIHPCCDY2uY7/HlmkxGgo3ozWBlBSRDeBqU65zgZD3QIPCxa6xaqB/Gc0bQ9BGzfU0cvVmO5jgNeeDnbqqs3oeA2jml/Qv2YO9upApfNQtDT1GiwJ8vrgaIow==
TestMailbox 00000180d8e513d3ea58c679a13178ac Ce5su2YOxNmTzk2dK8SX8V/Uue5uAC7oklEjhesY9wCMqGphhOkdWjzCqq0xOzcb/ZzzZ58t+mTksNSYIU4kddHIHBFPgqIwKthVk2mlUdqYiN/Y2vEGqv+YmtKY+GST/7Ee87ZHpU/5sv0GoXxT
TestMailbox 00000180d8e5145a23f8faee86283900 sp3D8xFZcM9icNlDJXIUDJb3mo6VGD9f1aDHD+4RbPdx6mTYF+qNTsPHKCxHHxT/9NfNe8XPg2+8xYRtm7SXfgERZBDB8ye+Xt3fM1k+wbL6RsaJmDHVECeXeL5KHuITzpI22A==
TestMailbox 00000180d8e51465c38f0585f9bb760e FF0VId2O/bBNzYD5ABWReMs5hHoHwynOoJRKj9vyaUMZ3JykInFmvvRgtCbJBDjTQPwPU8apphKQfwuicO76H7GtZqH009Cbv5l8ZTRJKrmzOQmtjzBQc2eGEUMPfbml5t0GCg==
```

The timestamp of a checkpoint corresponds to the timestamp of the first operation NOT included in the checkpoint.
In other words, to reconstruct the final state:

- find timestamp `<ts>` of last checkpoint
- load checkpoint `<ts>`
- load and apply all operations starting from `<ts>`, included

## UID index

The UID index is an application of the Bayou storage module
used to assign UID numbers to e-mails.
See document we sent to NGI for properties on UIDVALIDITY.

## Cryptography; key management

Keys that are used:

- master secret key (for indexes)
- curve25519 public/private key pair (for incoming mail)

Keys that are stored in K2V under PK `keys`:

- `public`: the public curve25519 key (plain text)
- `salt`: the 32-byte salt `S` used to calculate digests that index keys below
- if a password is used, `password:<truncated(128bit) argon2 digest of password using salt S>`:
  - a 32-byte salt `Skey`
  - followed a secret box
  - that is encrypted with a strong argon2 digest of the password (using the salt `Skey`) and a user secret (see below)
  - that contains the master secret key and the curve25519 private key

User secret: an additionnal secret that is added to the password when deriving the encryption key for the secret box.
This additionnal secret should not be stored in K2V/S3, so that just knowing a user's password isn't enough to be able
to decrypt their mailbox (supposing the attacker has a dump of their K2V/S3 bucket).
This user secret should typically be stored in the LDAP database or just in the configuration file when using
the static login provider.

Operations:

- **Initialize**(`user_secret`, `password`):
  - if `"salt"` or `"public"` already exist, BAIL
  - generate salt `S` (32 random bytes)
  - generate `public`, `private` (curve25519 keypair)
  - generate `master` (secretbox secret key)
  - calculate `digest = argon2_S(password)`
  - generate salt `Skey` (32 random bytes)
  - calculate `key = argon2_Skey(user_secret + password)`
  - serialize `box_contents = (private, master)`
  - seal box `blob = seal_key(box_contents)`
  - write `S` at `"salt"`
  - write `concat(Skey, blob)` at `"password:{hex(digest[..16])}"`
  - write `public` at `"public"`

- **InitializeWithoutPassword**(`private`, `master`):
  - if `"salt"` or `"public"` already exist, BAIL
  - generate salt `S` (32 random bytes)
  - write `S` at `"salt"`
  - calculate `public` the public key associated with `private`
  - write `public` at `"public"`

- **Open**(`user_secret`, `password`):
  - load `S = read("salt")`
  - calculate `digest = argon2_S(password)`
  - load `blob = read("password:{hex(digest[..16])}")
  - set `Skey = blob[..32]`
  - calculate `key = argon2_Skey(user_secret + password)`
  - open secret box `box_contents = open_key(blob[32..])`
  - retrieve `master` and `private` from `box_contents`
  - retrieve `public = read("public")`

- **OpenWithoutPassword**(`private`, `master`):
  - load `public = read("public")`
  - check that `public` is the correct public key associated with `private`

- **AddPassword**(`user_secret`, `existing_password`, `new_password`):
  - load `S = read("salt")`
  - calculate `digest = argon2_S(existing_password)`
  - load `blob = read("existing_password:{hex(digest[..16])}")
  - set `Skey = blob[..32]`
  - calculate `key = argon2_Skey(user_secret + existing_password)`
  - open secret box `box_contents = open_key(blob[32..])`
  - retrieve `master` and `private` from `box_contents`

  - calculate `digest_new = argon2_S(new_password)`
  - generate salt `Skeynew` (32 random bytes)
  - calculate `key_new = argon2_Skeynew(user_secret + new_password)`
  - serialize `box_contents_new = (private, master)`
  - seal box `blob_new = seal_key_new(box_contents_new)`
  - write `concat(Skeynew, blob_new)` at `"new_password:{hex(digest_new[..16])}"`

- **RemovePassword**(`password`):
  - load `S = read("salt")`
  - calculate `digest = argon2_S(existing_password)`
  - check that `"password:{hex(digest[..16])}"` exists
  - check that other passwords exist ?? (or not)
  - delete `"password:{hex(digest[..16])}"`
