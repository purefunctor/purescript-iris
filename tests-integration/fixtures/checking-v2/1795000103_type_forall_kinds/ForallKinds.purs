module ForallKinds where

class C :: forall k. k -> Constraint
class C a

-- The two field types differ only in the kind of the bound variable: Type vs Symbol.
x :: { f :: forall (a :: Type). C a => Int }
x = { f: 0 }

y :: { f :: forall (a :: Symbol). C a => Int }
y = { f: 1 }

-- Correct: the element types are not equal, so this must be rejected.
arr = [x, y]

-- If this compiles, y's field (polymorphic over symbols) is typed as polymorphic over types.
claim :: Array { f :: forall (a :: Type). C a => Int }
claim = arr
