use building_types::QueryResult;
use files::FileId;
use indexing::TermItemId;

use crate::native::NativeOperation;

use super::Context;

impl<Q> Context<'_, Q>
where
    Q: checking::ExternalQueries,
{
    pub(super) fn native_operation(
        &self,
        file_id: FileId,
        term_id: TermItemId,
    ) -> QueryResult<Option<NativeOperation>> {
        let module_name = self.source_module_name(file_id)?;
        let indexed = self.indexed_module(file_id)?;
        let Some(name) = indexed.items[term_id].name.as_deref() else {
            return Ok(None);
        };
        let operation = match (module_name.as_str(), name) {
            ("Iris.Effect.Sync", "pure") => Some(NativeOperation::SyncPure),
            ("Iris.Effect.Sync", "bind") => Some(NativeOperation::SyncBind),
            ("Iris.Effect.Sync", "discard") => Some(NativeOperation::SyncDiscard),
            ("Iris.Effect.Sync", "map") => Some(NativeOperation::SyncMap),
            ("Iris.Effect.Sync", "apply") => Some(NativeOperation::SyncApply),
            ("Iris.Effect.Sync", "abort") => Some(NativeOperation::SyncAbort),
            ("Iris.Effect.Sync", "catchAbort") => Some(NativeOperation::SyncCatchAbort),
            ("Iris.Effect.Compat", "liftEffect" | "liftEffectAs") => {
                Some(NativeOperation::SyncLiftEffect)
            }
            ("Iris.Effect.Async", "pure") => Some(NativeOperation::AsyncPure),
            ("Iris.Effect.Async", "bind") => Some(NativeOperation::AsyncBind),
            ("Iris.Effect.Async", "discard") => Some(NativeOperation::AsyncDiscard),
            ("Iris.Effect.Async", "map") => Some(NativeOperation::AsyncMap),
            ("Iris.Effect.Async", "apply") => Some(NativeOperation::AsyncApply),
            ("Iris.Effect.Async", "lift") => Some(NativeOperation::AsyncLift),
            ("Iris.Effect.Async", "defer") => Some(NativeOperation::AsyncDefer),
            ("Iris.Effect.Async", "yield") => Some(NativeOperation::AsyncYield),
            ("Iris.Effect.Async", "register") => Some(NativeOperation::AsyncRegister),
            ("Iris.Effect.Async", "bracket") => Some(NativeOperation::AsyncBracket),
            ("Iris.Effect.Async", "fromPromise") => Some(NativeOperation::AsyncFromPromise),
            ("Iris.Effect.Async", "run") => Some(NativeOperation::AsyncRun),
            ("Iris.Effect.Async", "join") => Some(NativeOperation::AsyncJoin),
            ("Iris.Effect.Async", "interrupt") => Some(NativeOperation::AsyncInterrupt),
            ("Iris.Effect.Async", "abort") => Some(NativeOperation::AsyncAbort),
            ("Iris.Effect.Async", "catchAbort") => Some(NativeOperation::AsyncCatchAbort),
            _ => None,
        };
        Ok(operation)
    }
}
