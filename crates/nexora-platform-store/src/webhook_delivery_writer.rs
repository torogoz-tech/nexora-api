use crate::{PlatformStore, WebhookProof};
use nexora_platform::PlatformError;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
pub(crate) struct WebhookDeliveryWriter;
impl WebhookDeliveryWriter {
    fn digest(proof: &WebhookProof, body: &[u8]) -> Vec<u8> {
        let mut hash = Sha256::new();
        hash.update(proof.event.as_bytes());
        hash.update([0]);
        hash.update(body);
        hash.finalize().to_vec()
    }
    pub fn duplicate(
        client: &mut impl postgres::GenericClient,
        namespace: &str,
        provider: &str,
        scope: &str,
        proof: &WebhookProof,
        body: &[u8],
    ) -> Result<bool, PlatformError> {
        let key = serde_json::to_string(&(namespace, provider, scope, &proof.delivery_id))
            .map_err(|_| PlatformError::Storage)?;
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
                &[&key],
            )
            .map_err(PlatformStore::database_error)?;
        let old=client.query_opt("SELECT body_digest FROM git_webhook_deliveries WHERE provider=$1 AND scope_id=$2 AND delivery_id=$3",&[&provider,&scope,&proof.delivery_id]).map_err(PlatformStore::database_error)?;
        if let Some(row) = old {
            let digest: Vec<u8> = row.get(0);
            if !bool::from(digest.ct_eq(&Self::digest(proof, body))) {
                return Err(PlatformError::Conflict(
                    "webhook delivery ID already has different content",
                ));
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub fn save(
        client: &mut impl postgres::GenericClient,
        provider: &str,
        scope: &str,
        proof: &WebhookProof,
        body: &[u8],
    ) -> Result<(), PlatformError> {
        client.execute("INSERT INTO git_webhook_deliveries(provider,scope_id,delivery_id,event,body_digest) VALUES($1,$2,$3,$4,$5)",&[&provider,&scope,&proof.delivery_id,&proof.event,&Self::digest(proof,body)]).map_err(PlatformStore::database_error)?;
        Ok(())
    }
}
