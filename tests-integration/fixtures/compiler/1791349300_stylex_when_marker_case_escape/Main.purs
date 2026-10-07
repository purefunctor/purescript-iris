module Main where

import Iris.StyleX (defineMarker)
import Iris.StyleX.When as When

marker = defineMarker

escaped = When.ancestorMarker ":hover" marker "red"
