#ifndef cWorld_h
#define cWorld_h
#include "cAvidaContext.h"
#include "cStats.h"
#include "cPopulation.h"
#include "avida/core/WorldDriver.h"
class cWorld {
public:
  cStats m_stats;
  cPopulation m_pop;
  Avida::WorldDriver m_driver;
  cAvidaContext* m_ctx;
  cWorld() : m_ctx(NULL) {}
  cStats& GetStats() { return m_stats; }
  cPopulation& GetPopulation() { return m_pop; }
  Avida::WorldDriver& GetDriver() { return m_driver; }
  cAvidaContext& GetDefaultContext() { return *m_ctx; }
};
#endif
