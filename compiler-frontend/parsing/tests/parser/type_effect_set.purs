module TypeEffectSet where

empty :: []

single :: [Database]

multiple :: [Database, Logger]

open :: [Database, Logger | effects]

polymorphic :: [forall a. a -> a]

openAppliedTail :: [Database | EffectRow effects]

application :: Effectful [Database] Result

nested :: [[Database], Logger | effects]
