module ImmediateCycle where

-- x and y refer to each other outside any function body.
x :: Int
x = y

y :: Int
y = x
