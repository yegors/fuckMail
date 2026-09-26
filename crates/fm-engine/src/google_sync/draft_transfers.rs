//! Complete a user-requested From-account move through the existing owned loop.
use super::{draft_actions, SyncContext};
use fm_core::Result;

/// Runs one due cleanup for a draft moved to another account: deletes the
/// old Google draft (or reconciles an earlier unknown deletion) and records
/// the result on the transfer. Returns `false` when nothing was due.
pub(super) async fn step(ctx: &SyncContext) -> Result<bool> {
    let account = ctx.account.as_str().to_owned();
    let due = ctx
        .core_call(move |store| store.draft_transfer_cleanup_due(&account))
        .await?;
    let Some(due) = due else { return Ok(false) };
    let result = if due.reconcile {
        draft_actions::reconcile(ctx, due.params["request_id"].as_str().unwrap()).await
    } else {
        draft_actions::delete(ctx, due.params).await
    };
    let account = ctx.account.as_str().to_owned();
    // The provider receipt survives cancellation; publish the corresponding
    // local transfer outcome through the same Core lifetime before returning.
    ctx.core
        .call(move |store| match result {
            Ok(status) => store.draft_transfer_cleanup_result(
                &account,
                &due.transfer_request_id,
                Some(&status),
                None,
            ),
            Err(error) => store.draft_transfer_cleanup_result(
                &account,
                &due.transfer_request_id,
                None,
                Some(&error),
            ),
        })
        .await?;
    Ok(true)
}
