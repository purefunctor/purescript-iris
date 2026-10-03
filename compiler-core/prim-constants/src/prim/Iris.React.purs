module Iris.React
  ( JSX
  , Component
  , component
  , element
  , elementKeyed
  , intrinsic
  , intrinsicKeyed
  , text
  , array
  , fragment
  , empty
  ) where

import Prim.Row as Row

data JSX :: Type
data JSX

data Component :: Row Type -> Type
data Component props
type role Component nominal

foreign import component
  :: forall props. (Record props -> JSX) -> Component props

foreign import element
  :: forall props. Row.Lacks "key" props
  => Component props -> Record props -> JSX

foreign import elementKeyed
  :: forall props. Row.Lacks "key" props
  => String -> Component props -> Record props -> JSX

foreign import intrinsic
  :: forall props. Row.Lacks "key" props
  => String -> Record props -> JSX

foreign import intrinsicKeyed
  :: forall props. Row.Lacks "key" props
  => String -> String -> Record props -> JSX

foreign import text :: String -> JSX
foreign import array :: Array JSX -> JSX
foreign import fragment :: Array JSX -> JSX
foreign import empty :: JSX
