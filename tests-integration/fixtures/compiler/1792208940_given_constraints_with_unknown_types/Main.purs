module Main where

import Data.Show (class Show, show)

givenRigid :: forall a. Show (Int String) => a -> String
givenRigid value = show value

givenConcrete :: Show (Int String) => String
givenConcrete = show 1

givenPartial :: forall a. Show (Array (Int String)) => Array a -> String
givenPartial value = show value
