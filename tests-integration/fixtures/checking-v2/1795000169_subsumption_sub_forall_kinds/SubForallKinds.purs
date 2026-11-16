module SubForallKinds where
class C :: forall k. k -> Constraint
class C a
x :: { f :: forall (a :: Type). C a => Int }
x = { f: 0 }
y :: { f :: forall (a :: Symbol). C a => Int }
y = { f: 1 }
useX :: { f :: forall (a :: Type). C a => Int } -> Int
useX _ = 0
-- PB-1.12's pair through subsumption instead of unification: must be rejected.
t = useX y
