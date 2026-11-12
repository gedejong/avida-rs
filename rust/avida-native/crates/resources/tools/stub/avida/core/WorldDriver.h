#ifndef AvidaWorldDriver_h
#define AvidaWorldDriver_h
#include "avida/core/Feedback.h"
namespace Avida {
  class WorldDriver {
    Avida::Feedback m_fb;
  public:
    Avida::Feedback& Feedback() { return m_fb; }
  };
}
#endif
