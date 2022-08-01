# Cryptography & key management

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
