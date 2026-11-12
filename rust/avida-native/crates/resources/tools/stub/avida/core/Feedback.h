#ifndef AvidaFeedback_h
#define AvidaFeedback_h
#include <string>
#include <vector>
namespace Avida {
  class Feedback {
  public:
    std::vector<std::string> errors;
    void Error(const char* fmt, ...) { errors.push_back(fmt); }
  };
}
#endif
