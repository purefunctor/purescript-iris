module Orphan where

import Class (class Describe)
import Subject (Subject)

instance Describe Subject where
  describe _ = "subject"
