#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeOperation {
    SyncPure,
    SyncBind,
    SyncDiscard,
    SyncMap,
    SyncApply,
    SyncAbort,
    SyncCatchAbort,
    AsyncPure,
    AsyncBind,
    AsyncDiscard,
    AsyncMap,
    AsyncApply,
    AsyncLift,
    AsyncDefer,
    AsyncYield,
    AsyncRegister,
    AsyncBracket,
    AsyncRun,
    AsyncJoin,
    AsyncInterrupt,
    AsyncAbort,
    AsyncCatchAbort,
}

impl NativeOperation {
    pub fn javascript_name(self) -> &'static str {
        match self {
            NativeOperation::SyncPure => "syncPure",
            NativeOperation::SyncBind => "syncBind",
            NativeOperation::SyncDiscard => "syncDiscard",
            NativeOperation::SyncMap => "syncMap",
            NativeOperation::SyncApply => "syncApply",
            NativeOperation::SyncAbort => "syncAbort",
            NativeOperation::SyncCatchAbort => "syncCatchAbort",
            NativeOperation::AsyncPure => "asyncPure",
            NativeOperation::AsyncBind => "asyncBind",
            NativeOperation::AsyncDiscard => "asyncDiscard",
            NativeOperation::AsyncMap => "asyncMap",
            NativeOperation::AsyncApply => "asyncApply",
            NativeOperation::AsyncLift => "asyncLift",
            NativeOperation::AsyncDefer => "asyncDefer",
            NativeOperation::AsyncYield => "asyncYield",
            NativeOperation::AsyncRegister => "asyncRegister",
            NativeOperation::AsyncBracket => "asyncBracket",
            NativeOperation::AsyncRun => "asyncRun",
            NativeOperation::AsyncJoin => "asyncJoin",
            NativeOperation::AsyncInterrupt => "asyncInterrupt",
            NativeOperation::AsyncAbort => "asyncAbort",
            NativeOperation::AsyncCatchAbort => "asyncCatchAbort",
        }
    }
}
