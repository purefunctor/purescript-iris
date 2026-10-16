-- @format width=80
-- @format width=100 indent=4
-- @format width=120
module Main (firstValue, secondValue, thirdValue, Collection(EmptyCollection, FullCollection)) where

import Data.Array (concat, filter, length, reverse, snoc, take, drop, zip, head)
import Data.String (contains, joinWith, length, split, take, drop, trim, toUpper)
import Library (firstValue, secondValue, thirdValue, Collection(EmptyCollection, FullCollection))
import Library hiding (firstValue, secondValue, thirdValue, Collection(EmptyCollection, FullCollection)) as Hidden
import Shapes (Shape(Circle, Square, Triangle, Rectangle, Pentagon, Hexagon, Octagon, Decagon))

firstValue = 1
secondValue = 2
thirdValue = 3
data Collection = EmptyCollection | FullCollection
