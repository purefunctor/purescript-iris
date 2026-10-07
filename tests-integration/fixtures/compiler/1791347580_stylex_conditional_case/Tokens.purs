module Tokens (breakpoints) where

import Iris.StyleX as StyleX

breakpoints = StyleX.defineConsts { small: "@media (max-width: 600px)" }
