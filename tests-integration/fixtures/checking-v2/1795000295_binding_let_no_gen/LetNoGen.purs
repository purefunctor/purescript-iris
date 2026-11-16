module LetNoGen where

k :: forall a b. a -> b -> a
k x _ = x

-- inferLetBinding does not call typesOf: is an unannotated let generalized?
test :: Int
test = let ident x = x in k (ident 1) (ident "s")
