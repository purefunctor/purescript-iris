module Main where

foreign import data Program :: Effects -> Type -> Type
foreign import data Database :: Type
foreign import data Logger :: Type

foreign import empty :: Program [] Unit
foreign import closed :: Program [Database, Logger] Unit
foreign import open :: forall effects. Program [Database, Logger | effects] Unit
