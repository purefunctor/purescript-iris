module Main where

compose f g x = f (g x)

infixr 9 compose as <<<

identity value = value

function left right = left

chain left right = left `function <<< identity` right
