module Secret (mkSecret, mkUnusedPair) where

import Unused (UnusedType(..))

data Private = Private

mkSecret :: Private
mkSecret = Private

mkUnusedPair :: { secret :: Private, other :: UnusedType }
mkUnusedPair = { secret: Private, other: UnusedType }
