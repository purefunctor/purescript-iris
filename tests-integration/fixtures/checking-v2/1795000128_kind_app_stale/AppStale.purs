module AppStale where

-- f (f x) needs f :: k -> k, so f :: (a -> b) -> (a -> b), x :: a -> b, y :: a.
type S f x y = f (f x) y

foreign import data F :: Type -> Symbol -> Type

-- Correct: rejected, since F :: Type -> Symbol -> Type is not k -> k.
-- Accepted, it expands to F (F Int) "s", where F Int :: Symbol -> Type is passed for a Type.
type Bad = S F Int "s"

-- A value whose type is the ill-kinded expansion.
foreign import v :: Bad
