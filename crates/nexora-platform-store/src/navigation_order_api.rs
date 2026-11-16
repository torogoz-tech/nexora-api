use crate::{PlatformStore, navigation_order_store::NavigationOrderStore};
use nexora_platform::{AuthContext, NavigationOrderChange, PlatformError};
use serde_json::Value;
impl PlatformStore {
    pub fn navigation_order(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        NavigationOrderStore::execute(self, ctx, None)
    }
    pub fn change_navigation_order(
        &mut self,
        ctx: &AuthContext,
        input: &NavigationOrderChange,
    ) -> Result<Value, PlatformError> {
        NavigationOrderStore::execute(self, ctx, Some(input))
    }
}
