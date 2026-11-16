module SynonymPartialArgument where

type Rewrite e f (g :: Type -> Type) = g e -> f (g e)

type OnArray (t :: (Type -> Type) -> Type) r = (onArray :: t Array | r)

type Discard (t :: (Type -> Type) -> Type) = Int

data Box (t :: (Type -> Type) -> Type) = Box

saturated :: forall e f. { | OnArray (Rewrite e f) () } -> Array e -> f (Array e)
saturated record = record.onArray

discarded :: Discard (Rewrite Int Array)
discarded = 0

unsaturated :: Box (Rewrite Int Array)
unsaturated = Box
