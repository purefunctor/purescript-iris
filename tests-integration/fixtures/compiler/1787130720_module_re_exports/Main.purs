module Main
  ( module Transitive
  , constructorValue
  , operatorValue
  , localCollision
  , foreignResult
  , hostileResult
  , measured
  , transitiveMarker
  ) where

import Transitive
  ( Option(Just)
  , append
  , await
  , class Measure
  , foreignValue
  , marker
  , measure
  , visible
  , (<>)
  )

constructorValue :: Option
constructorValue = Just 42

operatorValue :: Int
operatorValue = visible 23 <> 9

localCollision :: Int
localCollision = append

foreignResult :: Int
foreignResult = foreignValue

hostileResult :: Int
hostileResult = await

measured :: Int
measured = measure 41

transitiveMarker :: Int
transitiveMarker = marker
