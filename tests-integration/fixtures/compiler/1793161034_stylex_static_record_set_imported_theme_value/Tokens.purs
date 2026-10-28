module Tokens where

import Iris.StyleX as StyleX

card :: { color :: String, padding :: String }
card = StyleX.defineConsts { color: "navy", padding: "8px" }
