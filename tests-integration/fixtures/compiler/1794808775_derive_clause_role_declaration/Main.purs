module Main where

import Data.Newtype (class Newtype)

newtype Attached a = Attached a
  derive (Newtype)
type role Attached representational

newtype Between a = Between a
derive instance Newtype (Between a) _
type role Between representational

newtype Standalone a = Standalone a
type role Standalone representational
derive instance Newtype (Standalone a) _
