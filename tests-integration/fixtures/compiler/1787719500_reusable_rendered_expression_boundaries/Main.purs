module Main where

import Control.Applicative (pure)
import Control.Apply (apply)
import Control.Bind (bind)
import Data.Functor (map)
import Effect (Effect)

data Box = Box Int

foreign import constructEffect :: forall a. String -> a -> Effect a

foreign import observe :: String -> Int -> Int

foreign import observed
  :: { apply :: Int -> Int
     , value :: Int
     }

stableApplication :: (Int -> Int) -> Boolean -> Int
stableApplication function condition =
  function
    ( case condition of
        true -> observe "application-then" 1
        false -> observe "application-else" 2
    )

observedApplication :: Boolean -> Int
observedApplication condition =
  observed.apply
    ( case condition of
        true -> observe "observed-then" 3
        false -> observe "observed-else" 4
    )

stableArray :: Int -> Boolean -> Array Int
stableArray value condition =
  [ value
  , case condition of
      true -> observe "array-then" 5
      false -> observe "array-else" 6
  ]

observedArray :: Boolean -> Array Int
observedArray condition =
  [ observed.value
  , case condition of
      true -> observe "observed-array-then" 7
      false -> observe "observed-array-else" 8
  ]

stablePure :: Int -> Effect Int
stablePure value = pure value

stableMap :: (Int -> Int) -> Effect Int
stableMap function = map function (constructEffect "stable-map" 9)

observedMap :: Boolean -> Effect Int
observedMap _ = map observed.apply (constructEffect "observed-map" 10)

mixedApply :: Boolean -> Effect Int
mixedApply _ =
  apply
    (constructEffect "mixed-function" (\value -> observe "mixed-call" value))
    do
      value <- constructEffect "mixed-argument-first" 18
      constructEffect "mixed-argument-second" value

joinedEffect :: Boolean -> Array (Effect Int)
joinedEffect condition =
  [ if condition then
      map observed.apply (constructEffect "joined-then" 16)
    else
      map observed.apply (constructEffect "joined-else" 17)
  ]

joinedPattern :: Boolean -> Int
joinedPattern condition =
  case
    case condition of
      true -> Box (observe "pattern-then" 11)
      false -> Box (observe "pattern-else" 12)
  of
    Box value -> value
