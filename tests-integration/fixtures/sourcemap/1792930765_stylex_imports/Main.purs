module Main (Accent, spacing, styles) where

import Iris.StyleX.Types (Color)
import Iris.StyleX as StyleX
import Tokens (variables)

type Accent = StyleX.TypedValue Color String

spacing = StyleX.defineConsts { gap: "8px" }

styles = StyleX.create { root: { color: variables.accent, padding: spacing.gap } }
