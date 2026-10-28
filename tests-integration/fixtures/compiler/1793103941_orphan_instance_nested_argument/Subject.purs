module Subject where

import Class (class Describe)

data Subject = Subject

instance Describe (Array Subject) where
  describe _ = "subjects"
