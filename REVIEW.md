# Pending
- In src/diff/apted.rs on line 55: I don't think size is needed. code.rs ASTMetadata already has node_id -> subtree size map.
- In src/diff/apted.rs on line 43: We should remove all #[allow(dead_code)] annotations. Either they are not necessary or if the code is dead it should be removed.
- In src/diff/apted.rs on line 30: TreeNodeInfo should be refactored into code.rs::ASTMetadata. Compute the node info once and then store it in the metadata in a HashMap, keyed by node.id().

