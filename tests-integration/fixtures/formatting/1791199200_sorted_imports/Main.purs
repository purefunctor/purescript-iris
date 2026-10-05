-- @format width=80 crlf=true
-- @format width=20 indent=1
-- @format width=40 indent=4 unicode=true
module Main where -- Module explanation.

import Data.Tuple (Tuple(..)) {- Tuple explanation.
   Still attached to Tuple. -}
import Control.Monad as Monad
import Prelude (identity)
-- The open Prelude import.
import Prelude -- Prelude explanation.
import Data.Maybe (Maybe(..)) as Maybe
-- Array explanation.
import Data.Array (length)
import Prelude hiding (identity)
import Prelude as Functions

-- The declaration explanation.
test = Tuple (Maybe.Just (Functions.identity 1)) (length [1, 2])
