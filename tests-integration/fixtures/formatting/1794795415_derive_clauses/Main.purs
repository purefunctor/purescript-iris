-- @format width=80
-- @format width=100 indent=4
module Main where

data Colour = Red | Green | Blue derive (Eq, Ord)

data Direction = North | East | South | West
      derive (Eq)
  -- Representation for generic programming
  derive (Generic)

newtype Name = Name String derive (Eq, Ord) derive (Newtype)

data Void derive (Eq)

data Command = Initialize Configuration | Execute Request Environment | Shutdown Reason | Restart
  derive (Eq, Ord, Generic)

data Expression annotation = Literal annotation Int | Variable annotation String | Application
  derive (Prelude.Eq, Prelude.Ord, Functor, Data.Foldable.Foldable, Data.Traversable.Traversable, Generic)
