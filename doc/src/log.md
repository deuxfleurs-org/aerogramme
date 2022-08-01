# Mutation Log


Back to our data structure, we note that one major challenge with this project is to *correctly* handle mutable data.
With our current design, multiple processes can interact with the same mutable data without coordination, and we need a way to detect and solve conflicts.
Directly storing the result in a single k2v key would not work as we have no transaction or lock mechanism, and our state would be always corrupted.
Instead, we choose to record an ordered log of operations, ie. transitions, that each client can use locally to rebuild the state, each transition has its own immutable identifier.
This technique is sometimes referred to as event sourcing.

With this system, we can't have conflict anymore at Garage level, but conflicts at the IMAP level can still occur, like 2 processes assigning the same identifier to different emails.
We thus need a logic to handle these conflicts that is flexible enough to accommodate the application's specific logic.

Our solution is inspired by the work conducted by Terry et al. on [Bayou](https://dl.acm.org/doi/10.1145/224056.224070).
Clients fetch regularly the log from Garage, each entry is ordered by a timestamp and a unique identifier.
One of the 2 conflicting clients will be in the state where it has executed a log entry in the wrong order according to the specified ordering.
This client will need to roll back its changes to reapply the log in the same order as the others, and on conflicts, the same logic will be applied by all the clients to get, in the end, the same state.

**Command definitions**

The log is made of a sequence of ordered commands that can be run to get a deterministic state in the end.
We define the following commands:

`FLAG_ADD <email_uuid> <flag>` - Add a flag to the target email  
`FLAG_DEL <email_uuid> <flag>` - Remove a flag from a target email  
`MAIL_DEL <email_uuid>` - Remove an email  
`MAIL_ADD <email_uuid> <uid>` - Register an email in the mailbox with the given identifier  
`REMOTE <s3 url>` - Command is not directly stored here, instead it must be fetched from S3, see batching to understand why.

*Note: FLAG commands could be enhanced with a MODSEQ field similar to the uid field for the emails, in order to implement IMAP RFC4551. Adding this field would force us to handle conflicts on flags
the same way as on emails, as MODSEQ must be monotonically incremented but is reset by a uid-validity change. This is out of the scope of this document.* 

**A note on UUID**

When adding an email to the system, we associate it with a *universally unique identifier* or *UUID.*
We can then reference this email in the rest of the system without fearing a conflict or a race condition are we are confident that this UUID is unique.

We could have used the email hash instead, but we identified some benefits in using UUID.
First, sometimes a mail must be duplicated, because the user received it from 2 different sources, so it is more correct to have 2 entries in the system.
Additionally, UUIDs are smaller and better compressible than a hash, which will lead to better performances.

**Batching commands**

Commands that are executed at the same time can be batched together.
Let's imagine a user is deleting its trash containing thousands of emails.
Instead of writing thousands of log lines, we can append them in a single entry.
If this entry becomes big (eg. > 100 commands), we can store it to S3 with the `REMOTE` command.
Batching is important as we want to keep the number of log entries small to be able to fetch them regularly and quickly.

## Fixing conflicts in the operation log 

The log is applied in order from the last checkpoint.
To stay in sync, the client regularly asks the server for the last commands.

When the log is applied, our system must enforce the following invariants:

- For all emails e1 and e2 in the log, such as e2.order > e1.order, then e2.uid > e1.uid

- For all emails e1 and e2 in the log, such as e1.uuid == e2.uuid, then e1.order == e2.order

If an invariant is broken, the conflict is solved with the following algorithm and the `uidvalidity` value is increased.


```python
def apply_mail_add(uuid, imap_uid):
    if imap_uid < internalseq:
        uidvalidity += internalseq - imap_uid
    mails.insert(uuid, internalseq, flags=["\Recent"])
    internalseq = internalseq + 1
    uidnext = internalseq

def apply_mail_del(uuid):
    mails.remove(uuid)
    internalseq = internalseq + 1
```

A mathematical demonstration in Appendix D. shows that this algorithm indeed guarantees that under the same `uidvalidity`, different e-mails cannot share the same IMAP UID.

To illustrate, let us imagine two processes that have a first operation A in common, and then had a divergent state when one applied an operation B, and another one applied an operation C. For process 1, we have:

```python
# state: uid-validity = 1, uid_next = 1, internalseq = 1
(A) MAIL_ADD x 1
# state: uid-validity = 1, x = 1, uid_next = 2, internalseq = 2
(B) MAIL_ADD y 2
# state: uid-validity = 1, x = 1, y = 2, uid_next = 3, internalseq = 3
```

And for process 2 we have:

```python
# state: uid-validity = 1, uid_next = 1, internalseq = 1
(A) MAIL_ADD x 1
# state: uid-validity = 1, x = 1, uid_next = 2, internalseq = 2
(C) MAIL_ADD z 2
# state: uid-validity = 1, x = 1, z = 2, uid_next = 3, internalseq = 3
```

Suppose that a new client connects to one of the two processes after the conflicting operations have been communicated between them. They may have before connected either to process 1 or to process 2, so they might have observed either mail `y` or mail `z` with UID 2. The only way to make sure that the client will not be confused about mail UIDs is to bump the uidvalidity when the conflict is solved. This is indeed what happens with our algorithm: for both processes, once they have learned of the other's conflicting operation, they will execute the following set of operations and end in a deterministic state:

```python
# state: uid-validity = 1, uid_next = 1, internalseq = 1
(A) MAIL_ADD x 1
# state: uid-validity = 1, x = 1, uid_next = 2, internalseq = 2
(B) MAIL_ADD y 2
# state: uid-validity = 1, x = 1, y = 2, uid_next = 3, internalseq = 3
(C) MAIL_ADD z 2
# conflict detected !
# state: uid-validity = 2, x = 1, y = 2, z = 3, uid_next = 4, internalseq = 4
```

## A computed state for efficient requests

From a data structure perspective, a list of commands is very inefficient to get the current state of the mailbox.
Indeed, we don't want an `O(n)` complexity (where `n` is the number of log commands in the log) each time we want to know how many emails are stored in the mailbox.
<!--To address this issue, we plan to maintain a locally computed (rollbackable) state of the mailbox.-->
To address this issue, and thus query the mailbox efficiently, the MDA keeps an in-memory computed version of the logs, ie. the computed state.

**Mapping IMAP identifiers to email identifiers with B-Tree**

Core features of IMAP are synchronization and listing of emails.
Its associated command is `FETCH`, it has 2 parameters, a range of `uid` (or `seq`) and a filter.
For us, it means that we must be able to efficiently select a range of emails by their identifier, otherwise the user experience will be bad, and compute resources will be wasted.

We identified that by using an ordered map based on a B-Tree, we can satisfy this requirement in an optimal manner.
For example, Rust defines a [BTreeMap](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html) object in its standard library.
We define the following structure for our mailbox:

```rust
struct mailbox {
  emails: BTreeMap<ImapUid, (EmailUuid, Flags)>,
  flags: BTreeMap<Flag, BTreeMap<ImapUid, EmailUuid>>,
  name: String,
  uid_next: u32,
  uid_validity: u32,
  /* other fields */
}
```

This data structure allows us to efficiently select a range of emails by their identifier by walking the tree, allowing the server to be responsive to syncronisation request from clients.

**Checkpoints**

Having an in-memory computed state does not solve all the problems of operation on a log only, as 1) bootstrapping a fresh client is expensive as we have to replay possibly thousand of logs, and 2) logs would be kept indefinitely, wasting valuable storage resources.

As a solution to these limitations, the MDA regularly checkpoints the in-memory state. More specifically, it serializes it (eg. with MessagePack), compresses it (eg. with zstd), and then stores it on Garage through the S3 API.
A fresh client would then only have to download the latest checkpoint and the range of logs between the checkpoint and now, allowing swift bootstraping while retaining all of the value of the log model.

Old logs and old checkpoints can be garbage collected after a few days for example as long as 1) the most recent checkpoint remains, 2) that all the logs after this checkpoint remain and 3) that we are confident enough that no log before this checkpoint will appear in the future.

