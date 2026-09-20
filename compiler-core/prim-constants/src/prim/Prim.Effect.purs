module Prim.Effect where

data Abort :: Type -> Type

class Union :: Effects -> Effects -> Effects -> Constraint
class Union left right union | left right -> union

class Remove :: Type -> Effects -> Effects -> Constraint
class Remove effect input output | effect input -> output

class Subset :: Effects -> Effects -> Constraint
class Subset required allowed
