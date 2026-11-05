module Main where

import Iris.Effect (getService)

newtype Storage = Storage Int

ambiguous :: Record (local :: Storage, session :: Storage) -> Storage
ambiguous = getService
