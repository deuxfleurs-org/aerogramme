use anyhow::{bail, Result};
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
    pub ancestors: OrdMap<UniqueIdent, OrdSet<UniqueIdent>>,

    /// All nodes
    pub all_nodes: OrdSet<UniqueIdent>,
    /// Head nodes
    pub heads: OrdSet<UniqueIdent>,
    /// Origin nodes
    pub origins: OrdSet<UniqueIdent>,
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

    /// All HEAD events
    pub fn heads_vec(&self) -> Vec<UniqueIdent> {
        self.heads.clone().into_iter().collect()
    }

    /// Resolve a sync token
    pub fn resolve(&self, known: UniqueIdent) -> Result<OrdSet<UniqueIdent>> {
        let already_known = self.all_ancestors(known);

        // We can't capture all missing events if we are not connected
        // to all sinks of the graph, ie. if we don't already know all the sinks.
        if !self.origins.is_subset(already_known.clone()) {
            bail!("Not enough history to produce a correct diff, a full resync is needed");
        }

        // Missing items are all existing graph items from which
        // we removed all items known by the given node.
        // In other words, all values in all_nodes that are not in already_known.
        Ok(self.all_nodes.clone().relative_complement(already_known))
    }

    /// Find all ancestors of a given 
    fn all_ancestors(&self, known: UniqueIdent) -> OrdSet<UniqueIdent> {
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
    fn register(&mut self, ident: UniqueIdent, entry: IndexEntry) {
        // Insert item in the source of trust
        self.table.insert(ident, entry.clone());

        // Update the cache
        let (filename, _etag) = entry;
        self.idx_by_filename.insert(filename, ident);
    }

    /// Unregister a WebDAV item (delete, move)
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

    /// When an event is processed, update the synchronization DAG
    fn sync_dag(&mut self, child: &UniqueIdent, parents: &[UniqueIdent]) -> bool {
        // --- Update SUCCESSORS
        // All parents must exist in successors otherwise we can't accept item:
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

        // This event is also a future successor
        self.successors.insert(*child, ordset![]);

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
        let items = self.table.iter().map(|(ident, entry)| (*ident, entry.clone())).collect();

        // We keep only the head entries from the sync graph,
        // these entries will be used to initialize it back when deserializing
        let heads = self.heads_vec();

        // Finale serialization object
        let val = DavDagSerializedRepr { items, heads };
        val.serialize(serializer)
    }
}
