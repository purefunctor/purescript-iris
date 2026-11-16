module SubForallKindsUnused where
y :: { f :: forall (a :: Symbol). Int }
y = { f: 1 }
useX :: { f :: forall (a :: Type). Int } -> Int
useX _ = 0
-- The bound variable is unused, so instantiating a :: Symbol at any Type... is ill-kinded;
-- but Int works for every a :: Type, so the field types are interchangeable in effect.
t = useX y
