module Main where

data Box a = Box a

map :: forall a b. (a -> b) -> Box a -> Box b
map f (Box a) = Box (f a)

apply :: forall a b. Box (a -> b) -> Box a -> Box b
apply (Box f) (Box a) = Box (f a)

bind :: forall a b. Box a -> (a -> Box b) -> Box b
bind (Box a) f = f a

empty :: forall a. Box (Array a)
empty = Box []

count :: Array Int -> Int
count _ = 0

bindDo = do
  value <- Box "life"
--      $
  items <- empty
--      $
  let total = count items
--$
  Box total

bindAdo = ado
  value <- Box "life"
--      $
  items <- empty
--      $
  in count items
