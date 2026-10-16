-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main
  (longApplication,
   longRecord,
   branches,
   lambda,
   operators,
   constrained,
   Box(..))
  where

import Data.Collections
  (Collection,
   mapCollection,
   filterCollection,
   foldCollection,
   traverseCollection)
  as Collections
