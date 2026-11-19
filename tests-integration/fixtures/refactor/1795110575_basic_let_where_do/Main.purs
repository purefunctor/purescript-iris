module Main where

data Box a = Box a

bind :: forall a b. Box a -> (a -> Box b) -> Box b
bind (Box a) f = f a

map :: forall a b. (a -> b) -> Box a -> Box b
map f (Box a) = Box (f a)

apply :: forall a b. Box (a -> b) -> Box a -> Box b
apply (Box f) (Box a) = Box (f a)

letAndWhere :: Int -> Int
letAndWhere input =
  let
    doubled = step input -- trailing comment
    tripled = step doubled
    annotated :: Int
    annotated = tripled
  in
    combine annotated
  where
  step 0 = 1
  step n = n

  combine x =
    let
      offset = "ok"
    in
      keep x offset

  keep x _ = x

doAndAdo :: Box Int
doAndAdo = do
  x <- Box 1
  let
    first = x
    second = "hi"
  ado
    y <- Box first
    let
      third = second
    in
      pick y third

pick :: Int -> String -> Int
pick a _ = a
