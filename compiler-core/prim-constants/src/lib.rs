macro_rules! define {
    ($($const:ident, $path:expr, $name:expr $(;)?)+) => {
        $(
            pub const $const: &str = include_str!($path);
        )+

        pub const MODULE_MAP: &[(&str, &str)] = &[
            $(
                ($name, $const),
            )+
        ];
    };
}

define!(
    PRIM, "prim/Prim.purs", "Prim";
    PRIM_BOOLEAN, "prim/Prim.Boolean.purs", "Prim.Boolean";
    PRIM_COERCE, "prim/Prim.Coerce.purs", "Prim.Coerce";
    PRIM_EFFECT, "prim/Prim.Effect.purs", "Prim.Effect";
    PRIM_INT, "prim/Prim.Int.purs", "Prim.Int";
    PRIM_ORDERING, "prim/Prim.Ordering.purs", "Prim.Ordering";
    PRIM_ROW, "prim/Prim.Row.purs", "Prim.Row";
    PRIM_ROW_LIST, "prim/Prim.RowList.purs", "Prim.RowList";
    PRIM_SYMBOL, "prim/Prim.Symbol.purs", "Prim.Symbol";
    PRIM_TYPE_ERROR, "prim/Prim.TypeError.purs", "Prim.TypeError";
    IRIS_EFFECT, "prim/Iris.Effect.purs", "Iris.Effect";
    IRIS_EFFECT_SYNC, "prim/Iris.Effect.Sync.purs", "Iris.Effect.Sync";
    IRIS_EFFECT_ASYNC, "prim/Iris.Effect.Async.purs", "Iris.Effect.Async";
    IRIS_EFFECT_COMPAT, "prim/Iris.Effect.Compat.purs", "Iris.Effect.Compat";
    IRIS_STYLEX, "prim/Iris.StyleX.purs", "Iris.StyleX";
    IRIS_STYLEX_WHEN, "prim/Iris.StyleX.When.purs", "Iris.StyleX.When";
    IRIS_STYLEX_TYPES, "prim/Iris.StyleX.Types.purs", "Iris.StyleX.Types";
);
