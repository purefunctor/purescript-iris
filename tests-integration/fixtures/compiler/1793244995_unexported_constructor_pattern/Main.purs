module Main where

import Prelude

import Store as Store

unwrap :: Store.Counter -> Int
unwrap value = case value of
  Store.Counter count -> count
