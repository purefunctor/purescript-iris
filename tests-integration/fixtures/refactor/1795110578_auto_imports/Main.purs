module Main where

import Single (mkAlpha)
import Multi
  ( mkBeta
  , mkGamma
  )
import Shadowed (mkShadowed) as Sh
import HiddenMod hiding (Hidden)
import Producer (makeFresh, makeClashA, makeClashB)
import Secret (mkUnusedPair)

data Local = Local

example :: Int
example =
  let
    a = mkAlpha
    b = mkBeta
    g = mkGamma
    f = makeFresh
    c1 = makeClashA
    c2 = makeClashB
    s = Sh.mkShadowed
    h = mkHidden
    unimportable = mkUnusedPair
  in
    drop a b g f c1 c2 s h unimportable

drop :: forall a b c d e f g h i. a -> b -> c -> d -> e -> f -> g -> h -> i -> Int
drop _ _ _ _ _ _ _ _ _ = 0
