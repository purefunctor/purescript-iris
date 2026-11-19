module Producer where

import FreshA (Fresh, Clash, mkFresh, mkClashA)
import FreshB (Clash, mkClashB) as FB

makeFresh :: Fresh
makeFresh = mkFresh

makeClashA :: Clash
makeClashA = mkClashA

makeClashB :: FB.Clash
makeClashB = FB.mkClashB
