module Main where

import Prim.TypeError (class Fail, class Warn, Beside, Quote)

type Invalid = Int String

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b where
  convert :: a -> b

instance Convert Int Int where
  convert value = value

reportFailure :: forall b. Fail (Beside (Quote Invalid) (Quote b)) => b -> Int
reportFailure _ = 0

failsLater :: Invalid -> Int
failsLater value = reportFailure (convert value)

reportWarning :: forall b. Warn (Beside (Quote Invalid) (Quote b)) => b -> Int
reportWarning _ = 0

warnsLater :: Invalid -> Int
warnsLater value = reportWarning (convert value)
