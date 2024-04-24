use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use im::{OrdMap, OrdSet, ordset};

use aero_bayou::*;

use crate::unique_ident::{gen_ident, UniqueIdent};

/// Parents are only persisted in the event log,
/// not in the checkpoints.
pub type Token = UniqueIdent;
pub type Parents = Vec<Token>;
pub type SyncDesc = (Parents, Token);

pub type BlobId = UniqueIdent;
pub type Etag = String;
pub type FileName = String;
pub type IndexEntry = (BlobId, FileName, Etag);

#[derive(Clone, Default)]
pub struct DavDag {
    /// Source of trust
    pub table: OrdMap<BlobId, IndexEntry>,

    /// Indexes optimized for queries
    pub idx_by_filename: OrdMap<FileName, BlobId>,

    // ------------ Below this line, data is ephemeral, ie. not checkpointed

    /// Partial synchronization graph
    pub ancestors: OrdMap<Token, OrdSet<Token>>,

    /// All nodes
    pub all_nodes: OrdSet<Token>,
    /// Head nodes
    pub heads: OrdSet<Token>,
    /// Origin nodes
    pub origins: OrdSet<Token>,

    /// File change token by token
    pub change: OrdMap<Token, SyncChange>,
}

#[derive(Clone, Debug)]
pub enum SyncChange {
    Ok(FileName),
    NotFound(FileName),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DavDagOp {
    /// Merge is a virtual operation run when multiple heads are discovered
    Merge(SyncDesc),

    /// Add an item to the collection
    Put(SyncDesc, IndexEntry),

    /// Delete an item from the collection
    Delete(SyncDesc, BlobId),
}
impl DavDagOp {
    pub fn token(&self) -> Token {
        match self {
            Self::Merge((_, t)) => *t,
            Self::Put((_, t), _) => *t,
            Self::Delete((_, t), _) => *t,
        }
    }
}

impl DavDag {
    pub fn op_merge(&self) -> DavDagOp {
        DavDagOp::Merge(self.sync_desc())
    }

    pub fn op_put(&self, entry: IndexEntry) -> DavDagOp {
        DavDagOp::Put(self.sync_desc(), entry)
    }

    pub fn op_delete(&self, blob_id: BlobId) -> DavDagOp {
        DavDagOp::Delete(self.sync_desc(), blob_id)
    }

    // HELPER functions

    pub fn heads_vec(&self) -> Vec<Token> {
       self.heads.clone().into_iter().collect() 
    }

    /// A sync descriptor
    pub fn sync_desc(&self) -> SyncDesc {
        (self.heads_vec(), gen_ident())
    }

    /// Resolve a sync token
    pub fn resolve(&self, known: Token) -> Result<OrdSet<Token>> {
        let already_known = self.all_ancestors(known);

        // We can't capture all missing events if we are not connected
        // to all sinks of the graph,
        // ie. if we don't already know all the sinks,
        // ie. if we are missing so much history that 
        // the event log has been transformed into a checkpoint
        if !self.origins.is_subset(already_known.clone()) {
            bail!("Not enough history to produce a correct diff, a full resync is needed");
        }

        // Missing items are *all existing graph items* from which
        // we removed *all items known by the given node*.
        // In other words, all values in `all_nodes` that are not in `already_known`.
        Ok(self.all_nodes.clone().relative_complement(already_known))
    }

    /// Find all ancestors of a given node
    fn all_ancestors(&self, known: Token) -> OrdSet<Token> {
        let mut all_known: OrdSet<UniqueIdent> = OrdSet::new();
        let mut to_collect = vec![known];
        loop {
            let cursor = match to_collect.pop() {
                // Loop stops here
                None => break,
                Some(v) => v,
            };

            if all_known.insert(cursor).is_some() {
                // Item already processed
                continue
            }

            // Collect parents
            let parents = match self.ancestors.get(&cursor) {
                None => continue,
                Some(c) => c,
            };
            to_collect.extend(parents.iter());
        }
        all_known
    }

    // INTERNAL functions

    /// Register a WebDAV item (put, copy, move)
    fn register(&mut self, sync_token: Option<Token>, entry: IndexEntry) {
        let (blob_id, filename, _etag) = entry.clone();

        // Insert item in the source of trust
        self.table.insert(blob_id, entry);

        // Update the cache
        self.idx_by_filename.insert(filename.to_string(), blob_id);

        // Record the change in the ephemeral synchronization map
        if let Some(sync_token) = sync_token {
            self.change.insert(sync_token, SyncChange::Ok(filename));
        }
    }

    /// Unregister a WebDAV item (delete, move)
    fn unregister(&mut self, sync_token: Token, blob_id: &BlobId) {
        // Query the source of truth to get the information we
        // need to clean the indexes
        let (_blob_id, filename, _etag) = match self.table.get(blob_id) {
            Some(v) => v,
            // Element does not exist, return early
            None => return,
        };
        self.idx_by_filename.remove(filename);

        // Record the change in the ephemeral synchronization map
        self.change.insert(sync_token, SyncChange::NotFound(filename.to_string()));

        // Finally clear item from the source of trust
        self.table.remove(blob_id);
    }

    /// When an event is processed, update the synchronization DAG
    fn sync_dag(&mut self, sync_desc: &SyncDesc) {
        let (parents, child) = sync_desc;

        // --- Update ANCESTORS
        // We register ancestors as it is required for the sync algorithm
        self.ancestors.insert(*child, parents.iter().fold(ordset![], |mut acc, p| {
            acc.insert(*p);
            acc
        }));

        // --- Update ORIGINS
        // If this event has no parents, it's an origin
        if parents.is_empty() {
            self.origins.insert(*child);
        }

        // --- Update HEADS
        // Remove from HEADS this event's parents
        parents.iter().for_each(|par| { self.heads.remove(par); });

        // This event becomes a new HEAD in turn
        self.heads.insert(*child);
        
        // --- Update ALL NODES
        self.all_nodes.insert(*child);
    }
}

impl std::fmt::Debug for DavDag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DavDag\n")?;
        for elem in self.table.iter() {
            f.write_fmt(format_args!("\t{:?} => {:?}", elem.0, elem.1))?;
        }
        Ok(())
    }
}

impl BayouState for DavDag {
    type Op = DavDagOp;

    fn apply(&self, op: &Self::Op) -> Self {
        let mut new = self.clone();
    
        match op {
            DavDagOp::Put(sync_desc, entry) => {
                new.sync_dag(sync_desc);
                new.register(Some(sync_desc.1), entry.clone());
            },
            DavDagOp::Delete(sync_desc, blob_id) => {
                new.sync_dag(sync_desc);
                new.unregister(sync_desc.1, blob_id);
            },
            DavDagOp::Merge(sync_desc) => {
                new.sync_dag(sync_desc);
            }
        }

        new
    }
}

// CUSTOM SERIALIZATION & DESERIALIZATION
#[derive(Serialize, Deserialize)]
struct DavDagSerializedRepr {
    items: Vec<IndexEntry>,
    heads: Vec<UniqueIdent>,
}

impl<'de> Deserialize<'de> for DavDag {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val: DavDagSerializedRepr = DavDagSerializedRepr::deserialize(d)?;
        let mut davdag = DavDag::default();

        // Build the table + index
        val.items.into_iter().for_each(|entry| davdag.register(None, entry));

        // Initialize the synchronization DAG with its roots
        val.heads.into_iter().for_each(|ident| {
            davdag.heads.insert(ident);
            davdag.origins.insert(ident);
            davdag.all_nodes.insert(ident);
        });

        Ok(davdag)
    }
}

impl Serialize for DavDag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Indexes are rebuilt on the fly, we serialize only the core database
        let items = self.table.iter().map(|(_, entry)| entry.clone()).collect();

        // We keep only the head entries from the sync graph,
        // these entries will be used to initialize it back when deserializing
        let heads = self.heads_vec();

        // Finale serialization object
        let val = DavDagSerializedRepr { items, heads };
        val.serialize(serializer)
    }
}

// ---- TESTS ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base() {
        let mut state = DavDag::default();

        // Add item 1
        {
            let m = UniqueIdent([0x01; 24]);
            let ev = state.op_put((m, "cal.ics".into(), "321-321".into()));
            state = state.apply(&ev);

            assert_eq!(state.table.len(), 1);
            assert_eq!(state.resolve(ev.token()).unwrap().len(), 0);
        }

        // Add 2 concurrent items
        let (t1, t2) = {
            let blob1 = UniqueIdent([0x02; 24]);
            let ev1 = state.op_put((blob1, "cal2.ics".into(), "321-321".into()));

            let blob2 = UniqueIdent([0x01; 24]);
            let ev2 = state.op_delete(blob2);

            state = state.apply(&ev1);
            state = state.apply(&ev2);

            assert_eq!(state.table.len(), 1);
            assert_eq!(state.resolve(ev1.token()).unwrap(), ordset![ev2.token()]);

            (ev1.token(), ev2.token())
        };

        // Add later a new item
        {
            let blob3 = UniqueIdent([0x03; 24]);
            let ev = state.op_put((blob3, "cal3.ics".into(), "321-321".into()));

            state = state.apply(&ev);
            assert_eq!(state.table.len(), 2);
            assert_eq!(state.resolve(ev.token()).unwrap().len(), 0);
            assert_eq!(state.resolve(t1).unwrap(), ordset![t2, ev.token()]);
        }
    }
}
