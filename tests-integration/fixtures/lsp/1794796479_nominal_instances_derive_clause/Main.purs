module Main where

class Area a

data Box = Box
  derive (Area)

-- Derive clauses produce instances for the type they are attached to.
-- nominal instances type Main.Box
-- nominal instances class Main.Area
