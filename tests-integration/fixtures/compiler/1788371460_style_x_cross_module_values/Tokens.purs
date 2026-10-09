module Tokens (await, rowMarker, spacing, variables) where

import Iris.StyleX as StyleX

await :: StyleX.Style
await = StyleX.defaultMarker

spacing = StyleX.defineConsts { gap: "21px" }

variables = StyleX.defineVars { accent: "blue" }

rowMarker :: StyleX.Marker
rowMarker = StyleX.defineMarker
