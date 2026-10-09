module Main (gap, gapProps, spacing, variables) where

import Iris.StyleX as StyleX

gap = "21px"

sizes = { gap }

spacing = StyleX.defineConsts { gap: sizes.gap }

variables = StyleX.defineVars { accent: "blue" }

gapStyles = StyleX.create { root: { margin: gap, color: variables.accent } }

gapProps = StyleX.props gapStyles.root
