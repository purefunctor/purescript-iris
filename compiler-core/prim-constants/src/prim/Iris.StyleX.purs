module Iris.StyleX
  ( Style
  , Props
  , Attrs
  , Keyframes
  , Marker
  , ConditionalValue
  , ConditionalCase
  , Variable
  , TypedValue
  , create
  , props
  , attrs
  , recordProps
  , recordAttrs
  , conditional
  , conditionalValue
  , conditionalCase
  , keyframes
  , defineConsts
  , defineVars
  , createTheme
  , defineMarker
  , markerStyle
  , defaultMarker
  , viewTransitionClass
  , positionTry
  , firstThatWorks
  ) where

import Prim.Row as Row
import Prim.RowList as RowList

data Style :: Type
data Style

type Props = { className :: String }

data Attrs :: Type
data Attrs

data Keyframes :: Type
data Keyframes

data Marker :: Type
data Marker

data ConditionalValue :: Type -> Type
data ConditionalValue value

data ConditionalCase :: Type -> Type
data ConditionalCase value

data Variable :: Type -> Type
data Variable definition

data TypedValue :: Type -> Type -> Type
data TypedValue syntax value

class CompileStyles :: Row Type -> Row Type -> Constraint
class CompileStyles input output | input -> output

class CompileStyleList :: RowList.RowList Type -> Row Type -> Constraint
class CompileStyleList input output | input -> output

class PropsInput :: Type -> Constraint

class CompileProps :: Row Type -> Row Type -> Constraint
class CompileProps input output | input -> output

class CompilePropsList :: RowList.RowList Type -> Row Type -> Constraint
class CompilePropsList input output | input -> output

class CompileAttrs :: Row Type -> Row Type -> Constraint
class CompileAttrs input output | input -> output

class CompileAttrsList :: RowList.RowList Type -> Row Type -> Constraint
class CompileAttrsList input output | input -> output

class CompileVars :: Row Type -> Row Type -> Constraint
class CompileVars input output | input -> output

class CompileVarList :: RowList.RowList Type -> Row Type -> Constraint
class CompileVarList input output | input -> output

class ThemeOverrides :: Row Type -> Row Type -> Constraint

class ThemeOverrideList :: RowList.RowList Type -> Row Type -> Constraint

instance
  ( RowList.RowToList input inputList
  , CompileStyleList inputList output
  ) =>
  CompileStyles input output

instance CompileStyleList RowList.Nil ()

instance
  ( CompileStyleList tail outputTail
  , Row.Cons label Style outputTail output
  ) =>
  CompileStyleList (RowList.Cons label (Record declarations) tail) output

instance PropsInput Style

instance PropsInput (Array Style)

instance PropsInput Marker

instance
  ( RowList.RowToList input inputList
  , CompilePropsList inputList output
  ) =>
  CompileProps input output

instance CompilePropsList RowList.Nil ()

instance
  ( CompilePropsList tail outputTail
  , Row.Cons label Props outputTail output
  ) =>
  CompilePropsList (RowList.Cons label Style tail) output

instance
  ( RowList.RowToList input inputList
  , CompileAttrsList inputList output
  ) =>
  CompileAttrs input output

instance CompileAttrsList RowList.Nil ()

instance
  ( CompileAttrsList tail outputTail
  , Row.Cons label Attrs outputTail output
  ) =>
  CompileAttrsList (RowList.Cons label Style tail) output

instance
  ( RowList.RowToList input inputList
  , CompileVarList inputList output
  ) =>
  CompileVars input output

instance CompileVarList RowList.Nil ()

instance
  ( CompileVarList tail outputTail
  , Row.Cons label (Variable definition) outputTail output
  ) =>
  CompileVarList (RowList.Cons label definition tail) output

instance
  ( RowList.RowToList overrides overrideList
  , ThemeOverrideList overrideList variables
  ) =>
  ThemeOverrides variables overrides

instance ThemeOverrideList RowList.Nil variables

instance
  ( Row.Cons label (Variable definition) variablesTail variables
  , ThemeOverrideList tail variables
  ) =>
  ThemeOverrideList (RowList.Cons label definition tail) variables

foreign import create
  :: forall input output
   . CompileStyles input output
  => Record input
  -> Record output

foreign import props
  :: forall input
   . PropsInput input
  => input
  -> Props

foreign import attrs
  :: forall input
   . PropsInput input
  => input
  -> Attrs

foreign import recordProps
  :: forall input output
   . CompileProps input output
  => Record input
  -> Record output

foreign import recordAttrs
  :: forall input output
   . CompileAttrs input output
  => Record input
  -> Record output

foreign import conditional :: Boolean -> Style -> Style

foreign import conditionalValue
  :: forall value
   . value
  -> Array (ConditionalCase value)
  -> ConditionalValue value

-- | A `conditionalValue` case keyed by a condition string such as a media query, including a
-- | `defineConsts` value, which record labels cannot express.
foreign import conditionalCase :: forall value. String -> value -> ConditionalCase value

foreign import keyframes :: forall frames. Record frames -> Keyframes

foreign import defineConsts :: forall constants. Record constants -> Record constants

foreign import defineVars
  :: forall input output
   . CompileVars input output
  => Record input
  -> Record output

foreign import createTheme
  :: forall variables overrides
   . ThemeOverrides variables overrides
  => Record variables
  -> Record overrides
  -> Style

foreign import defineMarker :: Marker

-- | Include a named marker in a style array passed to `props` or `attrs`.
-- | The conversion is erased; keep the original `Marker` for `When.*Marker`.
foreign import markerStyle :: Marker -> Style

foreign import defaultMarker :: Style

foreign import viewTransitionClass :: forall options. Record options -> String

foreign import positionTry :: forall declarations. Record declarations -> String

foreign import firstThatWorks :: forall value. Array value -> value
