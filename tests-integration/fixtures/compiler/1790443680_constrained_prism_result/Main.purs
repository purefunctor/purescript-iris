module Main where

import Prelude

import Data.Either (Either(..))
import Data.Lens.Prism (Prism, below, matching, prism)
import Data.Lens.Prism.Maybe (_Just, _Nothing)
import Data.Maybe (Maybe(..), maybe)

test :: forall a b. Prism (Maybe a) (Maybe b) a b
test = prism Just $ maybe (Left Nothing) Right

test' :: forall a b. Prism (Maybe a) (Maybe b) a b
test' = prism Just (maybe (Left Nothing) Right)

test2 :: forall a b. Prism (Maybe a) (Maybe b) Unit Unit
test2 = prism (const Nothing) $ maybe (Right unit) (const $ Left Nothing)

results :: Array Boolean
results =
  [ matching (below test) [Just 3, Just 8] == Right [3, 8]
  , matching (below test') [Just 3, Just 8] == Right [3, 8]
  , matching (below _Just) [Just 3, Just 8] == Right [3, 8]
  , matching (below test) [Just 3, Nothing] == Left [Just 3, Nothing]
  , matching test2 (Nothing :: Maybe Int) == (Right unit :: Either (Maybe Int) Unit)
  , matching test2 (Just 8) == Left (Nothing :: Maybe Int)
  , matching _Nothing (Nothing :: Maybe Int) == (Right unit :: Either (Maybe Int) Unit)
  ]
