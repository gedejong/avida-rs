#ifndef cAvidaContext_h
#define cAvidaContext_h
#include "avida/Avida.h"
#include "avida/core/WorldDriver.h"
class cAvidaContext {
  Avida::WorldDriver* m_driver;
  Apto::Random* m_rng;
public:
  cAvidaContext(Avida::WorldDriver* driver, Apto::Random& rng) : m_driver(driver), m_rng(&rng) {}
  Apto::Random& GetRandom() { return *m_rng; }
  bool HasDriver() const { return (m_driver != NULL); }
};
#endif
