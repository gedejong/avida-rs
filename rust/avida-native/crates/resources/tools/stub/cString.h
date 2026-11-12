#ifndef cString_h
#define cString_h
#include "avida/Avida.h"
#include <string>
#include <iostream>
class cString {
  std::string s;
public:
  cString() {}
  cString(const char* c) : s(c) {}
  bool operator==(const cString& o) const { return s == o.s; }
  operator const char*() const { return s.c_str(); }
  friend std::ostream& operator<<(std::ostream& os, const cString& c) { return os << c.s; }
};
#endif
