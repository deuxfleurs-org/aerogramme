use serde::{Deserialize, Deserializer, Serialize, Serializer};
use im::{OrdMap, OrdSet, ordset};

use aero_bayou::*;

use crate::unique_ident::UniqueIdent;

/// Parents are only persisted in the event log,
/// not in the checkpoints.
pub type Parents = Vec<UniqueIdent>;
pub type Etag = String;
pub type FileName = String;
pub type IndexEntry = (FileName, Etag);

#[derive(Clone, Default)]
pub struct DavDag {
    /// Source of trust
    pub table: OrdMap<UniqueIdent, IndexEntry>,

    /// Indexes optimized for queries
    pub idx_by_filename: OrdMap<FileName, UniqueIdent>,

    /// Partial synchronization graph
    /// parent -> direct children
    pub successors: OrdMap<UniqueIdent, OrdSet<UniqueIdent>>,
    
    /// Head nodes
    pub heads: OrdSet<UniqueIdent>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DavDagOp {
    /// Merge is a virtual operation run when multiple heads are discovered
    Merge(Parents, UniqueIdent),

    /// Add an item to the collection
    Put(Parents, UniqueIdent, IndexEntry),

    /// Delete an item from the collection
    Delete(Parents, UniqueIdent),
}

impl DavDag {
    pub fn op_merge(&self, ident: UniqueIdent) -> DavDagOp {
        DavDagOp::Merge(self.heads_vec(), ident)
    }

    pub fn op_put(&self, ident: UniqueIdent, entry: IndexEntry) -> DavDagOp {
        DavDagOp::Put(self.heads_vec(), ident, entry)
    }

    pub fn op_delete(&self, ident: UniqueIdent) -> DavDagOp {
        DavDagOp::Delete(self.heads_vec(), ident)
    }

    // HELPER functions
    pub fn heads_vec(&self) -> Vec<UniqueIdent> {
        self.heads.clone().into_iter().collect()
    }

    // INTERNAL functions
    fn register(&mut self, ident: UniqueIdent, entry: IndexEntry) {
        // Insert item in the source of trust
        self.table.insert(ident, entry.clone());

        // Update the cache
        let (filename, _etag) = entry;
        self.idx_by_filename.insert(filename, ident);
    }

    fn unregister(&mut self, ident: &UniqueIdent) {
        // Query the source of truth to get the information we
        // need to clean the indexes
        let (filename, _etag) = match self.table.get(ident) {
            Some(v) => v,
            None => return,
        };
        self.idx_by_filename.remove(filename);

        // Finally clear item from the source of trust
        self.table.remove(ident);
    }

    // @FIXME: maybe in case of error we could simply disable the sync graph
    // and ask the client to rely on manual sync. For now, we are skipping the event
    // which is midly satisfying.
    fn sync_dag(&mut self, child: &UniqueIdent, parents: &[UniqueIdent]) -> bool {
        // All parents must exist in successors otherwise we can't accept item
        // do the check + update successors
        let mut try_successors = self.successors.clone();
        for par in parents.iter() {
            match try_successors.get_mut(par) {
                None => {
                    tracing::warn!("Unable to push a Dav DAG sync op into the graph, an event is missing, it's a bug");
                    return false
                },
                Some(v) => v.insert(*child),
            };
        }
        self.successors = try_successors;

        // Remove from HEADS this event's parents
        parents.iter().for_each(|par| { self.heads.remove(par); });

        // This event becomes a new HEAD in turn
        self.heads.insert(*child);

        // This event is also a future successor
        self.successors.insert(*child, ordset![]);

        true
    }
}

impl BayouState for DavDag {
    type Op = DavDagOp;

    fn apply(&self, op: &Self::Op) -> Self {
        let mut new = self.clone();
    
        match op {
            DavDagOp::Put(parents, ident, entry) => {
                if new.sync_dag(ident, parents.as_slice()) {
                    new.register(*ident, entry.clone());
                }
            },
            DavDagOp::Delete(parents, ident) => {
                if new.sync_dag(ident, parents.as_slice()) {
                    new.unregister(ident);
                }
            },
            DavDagOp::Merge(parents, ident) => {
                new.sync_dag(ident, parents.as_slice());
            }
        }

        new
    }
}

// CUSTOM SERIALIZATION & DESERIALIZATION
#[derive(Serialize, Deserialize)]
struct DavDagSerializedRepr {
    items: Vec<(UniqueIdent, IndexEntry)>,
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
        val.items.into_iter().for_each(|(ident, entry)| davdag.register(ident, entry));

        // Initialize the synchronization DAG with its roots
        val.heads.into_iter().for_each(|ident| {
            davdag.successors.insert(ident, ordset![]);
            davdag.heads.insert(ident);
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
        let items = self.table.iter().map(|(ident, entry)| (*ident, entry.clone())).collect();

        // We keep only the head entries from the sync graph,
        // these entries will be used to initialize it back when deserializing
        let heads = self.heads_vec();

        // Finale serialization object
        let val = DavDagSerializedRepr { items, heads };
        val.serialize(serializer)
    }
}
