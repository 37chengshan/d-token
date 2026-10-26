//! Provider-neutral conversation inventory.
//!
//! This module is the migration boundary for inventory code that was first
//! written for Optical.  The inventory itself only describes roles, tool
//! correlations, opaque parts and candidate eligibility; it does not render
//! or mutate images.

pub use crate::optical::inventory::{
    build_anthropic_inventory, build_anthropic_inventory_with_arena, build_openai_chat_inventory,
    build_openai_chat_inventory_with_arena, build_openai_responses_inventory,
    build_openai_responses_inventory_with_arena, BlockKind, BlockRef, ConversationInventory,
    ConversationTurn, InitialProtection, RoleKind,
};

// Provider-neutral unit inventory belongs to the local context boundary. The
// legacy wire inventories above remain available for existing adapters.
pub use crate::protection::{
    inventory_context, ContextInventory, ContextInventoryEntry, ContextUnit, ContextUnitKind,
};
