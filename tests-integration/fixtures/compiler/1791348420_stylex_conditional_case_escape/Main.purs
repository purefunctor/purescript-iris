module Main where

import Iris.StyleX as StyleX

escaped = StyleX.conditionalCase "@media print" "red"
