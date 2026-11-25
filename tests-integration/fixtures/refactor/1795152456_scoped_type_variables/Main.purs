module Main where

data Box a = Box a

type Pair a = Box a

identity :: forall a. a -> a
identity = go
  where
  go a = a

visibleOuter :: forall @a. a -> a
visibleOuter = go
  where
  go x = x

class BoxLike f where
  boxMap :: forall a b. (a -> b) -> f a -> f b

mapScoped :: forall f a. BoxLike f => (a -> a) -> f a -> f a
mapScoped g x = go x
  where
  go y = boxMap g y

nestedForall :: forall a. a -> a
nestedForall outerValue = inner outerValue
  where
  inner :: forall b. b -> b
  inner innerValue = go innerValue
    where
    go z = z

shadowedOuter :: forall a. a -> Int
shadowedOuter outerValue = inner outerValue
  where
  inner :: forall a. a -> Int
  inner innerValue =
    let
      fromOuter = outerValue
      fromInner = innerValue
      _ = fromOuter
      _ = fromInner
    in
      0

class Wrap a where
  wrapIt :: a -> a

instance Wrap (Pair elem) where
  wrapIt value =
    let
      same = value
    in
      same

class WithPoly a where
  withPoly :: forall b. a -> b -> a

instance WithPoly Int where
  withPoly x y =
    let
      keepY = y
      _ = keepY
    in
      x

instance WithPoly String where
  withPoly :: forall b. String -> b -> String
  withPoly x y =
    let
      keepY = y
      _ = keepY
    in
      x

unannotatedTop = go
  where
  go x = x

afterArrow :: Int -> forall a. a -> a
afterArrow _ x = go x
  where
  go y = y

afterConstraint :: forall a. Wrap a => forall b. a -> b -> a
afterConstraint x y =
  let
    keepA = x
    keepB = y
    _ = keepB
  in
    keepA

type Endo = forall a. a -> a

viaSynonym :: Endo
viaSynonym x = go x
  where
  go y = y

runRank2 :: (forall a. a -> a) -> Int
runRank2 _ = 0

useRank2 :: Int
useRank2 =
  runRank2 \x ->
    let
      y = x
    in
      y

unusedUnsolved :: Int
unusedUnsolved =
  let
    unused x = x
  in
    0

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

takePoly :: forall k (x :: k). Proxy x -> (forall (y :: k). Proxy y -> Int)
takePoly _ _ = 0

sameForallShadowed :: forall k (x :: k) k. Proxy x -> Proxy k -> Int
sameForallShadowed px _ =
  let
    shadowedKind = takePoly px
    _ = shadowedKind
  in
    0
