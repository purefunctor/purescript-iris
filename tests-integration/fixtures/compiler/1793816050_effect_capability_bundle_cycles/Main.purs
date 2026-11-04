module Main where

import Iris.Effect (class Has, type (&))

data Service

type Recursive = Service & Recursive

type Growing value = Array (Growing (Array value))

recursive :: forall services. Has Recursive services => Record services -> Int
recursive _ = 0

growing :: Growing Int -> Int
growing _ = 0
