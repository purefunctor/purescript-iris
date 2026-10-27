module Subject where

import Class (class Convert)

data Subject = Subject

instance Convert Subject Int where
  convert _ = 0

instance Convert Int Subject where
  convert _ = Subject
