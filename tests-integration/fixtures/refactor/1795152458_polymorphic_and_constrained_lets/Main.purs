module Main where

import Helper (Box(..), boxMap, pickFirst)

applyTo :: forall a. a -> (forall b. (a -> b) -> b)
applyTo x f = f x

visibleIdentity :: forall @a. a -> a
visibleIdentity x = x

bodyCapture :: forall a. a -> Int
bodyCapture x =
  let
    pick = pickFirst
      where
      keep = x
      _ = keep
  in
    pick 1 "hi"

directCapture :: forall b. b -> b
directCapture x =
  let
    apply = applyTo x
  in
    apply \y -> y

constrainedAlias :: Box Int -> Box Int
constrainedAlias x =
  let
    localMap = boxMap
  in
    localMap (\n -> n) x

class LocalLike f where
  localLikeMap :: forall a b. (a -> b) -> f a -> f b

constrainedShadowing :: forall f a. LocalLike f => f a -> f a
constrainedShadowing x =
  let
    localMap = localLikeMap
    mapped = localMap (\y -> y) x
  in
    mapped

visibleAlias :: forall a. a -> a
visibleAlias x =
  let
    localVis = visibleIdentity
  in
    localVis @a x
