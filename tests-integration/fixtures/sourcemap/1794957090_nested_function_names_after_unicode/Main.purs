module Main where

import Prelude

outer :: Boolean -> String -> String
outer flag =
  let sélection value = if flag then value else "else"
  in \value -> sélection "😀" <> sélection value
