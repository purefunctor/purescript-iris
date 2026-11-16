module LevelResetUndefined where

import KProxy (Proxy)

foreign import data P :: forall k (a :: k). (k -> Type) -> Type
foreign import data Q :: forall j. j -> Type

data D :: forall x. Proxy (P Q) -> Proxy x -> Type
data D p q = D

foreign import data R :: Proxy (P Q)
foreign import data S :: Proxy Int

-- R fits D's first argument; purs instead reports that D's own kind variable k2 is undefined.
-- (Without the forall, this synonym is rejected as ambiguous, QuantificationCheckFailureInType,
-- because R's kind arguments are never fixed; see LevelResetUse for the version with a control.)
type Y = D R S
