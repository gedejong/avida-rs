#ifndef cResource_h
#define cResource_h
#include "cString.h"
// Only what cResourceCount / cSpatialResCount use from cResource.h at 47f13dadb.
class cResource {
public:
  static const int NONE = -99;
};
class cCellResource {
  int cell_id; double initial; double inflow; double outflow;
public:
  cCellResource() : cell_id(cResource::NONE), initial(0.0), inflow(0.0), outflow(0.0) {}
  cCellResource(int _cell_id, double _initial, double _inflow, double _outflow)
    : cell_id(_cell_id), initial(_initial), inflow(_inflow), outflow(_outflow) {}
  int GetId() const { return cell_id; }
  double GetInitial() const { return initial; }
  double GetInflow() const { return inflow; }
  double GetOutflow() const { return outflow; }
};
#endif
