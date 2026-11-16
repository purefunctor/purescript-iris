module AppStaleControl where

-- The same constraint without a third application: f :: k -> k.
type S f x = f (f x)

foreign import data F :: Type -> Symbol -> Type

-- Rejected: F is not k -> k.
type Bad = S F Int
