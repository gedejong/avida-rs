// Reference generator for src/fmt.rs (C++ iostream / printf number formatting).
//
//   c++ -std=c++11 -O2 rust/avida-native/crates/apto/tools/fmt_ref.cc -o /tmp/fmt_ref
//   /tmp/fmt_ref > rust/avida-native/crates/apto/testdata/fmt_ref.txt
//   /tmp/fmt_ref gsweep > rust/avida-native/crates/apto/testdata/fmt_g_sweep.txt
//
// One line per double, tab separated: the bit pattern in hex, then each
// format in FORMATS order (printf specs), then the ostream outputs:
// default `os << v`, `showpoint + setprecision(7)`, and the driver status-line
// cell `left + showpoint + setw(9) + setprecision(7)`. %f-style columns are
// "-" for |v| >= 1e22 to keep the file small.
//
// `gsweep` prints, per double, %.0g through %.17g. Its values concentrate on
// integers whose digits end in an exact rounding tie (d...d0|5000), where
// macOS libc's dtoa keeps trailing zeros that %g would otherwise trim.

#include <cmath>
#include <cstdio>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <limits>
#include <sstream>
#include <vector>

static const char* FORMATS[] = {
  "%g", "%.1g", "%.3g", "%.7g", "%.10g", "%.17g", "%#.7g", "%#g", "%#.1g", "%-12.4g", "%+g",
  "%e", "%.0e", "%7.1e", "%#.0e",
  "%f", "%.0f", "%.2f", "%7.2f", "%04.3f", "%1.5f", "%#.0f", "%-9.3f",
};
static const int NUM_FORMATS = sizeof(FORMATS) / sizeof(FORMATS[0]);

static unsigned long long s_state = 0x9E3779B97F4A7C15ULL;
static unsigned long long splitmix()
{
  unsigned long long z = (s_state += 0x9E3779B97F4A7C15ULL);
  z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
  z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
  return z ^ (z >> 31);
}

static double FromBits(unsigned long long u) { double d; memcpy(&d, &u, sizeof(d)); return d; }

static int GSweep()
{
  std::vector<double> vals;
  // Integers d...d|5 0...0 (ties) and neighbours, around every precision.
  for (int i = 0; i < 1500; i++) {
    const int p = 1 + (int)(splitmix() % 16);           // kept digits
    const int tail = 1 + (int)(splitmix() % 4);          // digits after the cut
    double head = (double)(splitmix() % 9 + 1);
    for (int d = 1; d < p; d++) head = head * 10.0 + (double)(splitmix() % 10);
    if (splitmix() % 2) head = floor(head / 10.0) * 10.0;  // kept part ends in 0
    double v = head * pow(10.0, tail) + 5.0 * pow(10.0, tail - 1);
    const int nudge = (int)(splitmix() % 4);
    if (nudge == 1) v += 1.0;
    if (nudge == 2) v -= 1.0;
    if (splitmix() % 2) v = -v;
    vals.push_back(v);
  }
  for (int i = 0; i < 300; i++) vals.push_back((double)(splitmix() % 1000000000000000ULL));
  for (int i = 0; i < 200; i++) vals.push_back(FromBits(splitmix()));
  for (int i = 0; i < 200; i++) vals.push_back((double)(splitmix() % 2000000) / 64.0);

  char buf[512];
  for (size_t i = 0; i < vals.size(); i++) {
    unsigned long long u;
    memcpy(&u, &vals[i], sizeof(u));
    printf("%016llx", u);
    for (int p = 0; p <= 17; p++) {
      snprintf(buf, sizeof(buf), "%.*g", p, vals[i]);
      printf("\t%s", buf);
    }
    printf("\n");
  }
  return 0;
}

int main(int argc, char** argv)
{
  if (argc > 1 && strcmp(argv[1], "gsweep") == 0) return GSweep();
  std::vector<double> vals;
  const double edge[] = {
    0.0, -0.0, 1.0, -1.0, 0.5, 1.5, 2.5, 0.125, 0.05, 0.15, 0.25, 0.35, 1e-5, 1e-4, 0.0001, 0.00012345,
    123456.0, 1234567.0, 999999.0, 999999.5, 9999995.0, 0.999999, 0.9999995, 9.9999995, 99.99995,
    1e6, 1e7, 1e15, 1e16, 1e17, 1e21, 1e22, 1e100, 1e300, 1e-300, 1e-310, 5e-324, 1.7976931348623157e308,
    2.2250738585072014e-308, 3.14159265358979, 2.718281828459045, 100.0, 100.5, 0.1, 0.2, 0.3, 1.0 / 3.0,
    2.0 / 3.0, 0.0625, 1048576.0, 4294967296.0, 65.5, 1.05, 1.005, 1.0005, 0.045, 0.0045, 12.345, 123.45,
    std::numeric_limits<double>::infinity(), -std::numeric_limits<double>::infinity(),
    std::numeric_limits<double>::quiet_NaN(), -std::numeric_limits<double>::quiet_NaN(),
  };
  for (size_t i = 0; i < sizeof(edge) / sizeof(edge[0]); i++) vals.push_back(edge[i]);

  // Arbitrary bit patterns (all magnitudes).
  for (int i = 0; i < 800; i++) vals.push_back(FromBits(splitmix()));
  // Typical data-file magnitudes: k / 10^n and ratios.
  for (int i = 0; i < 1000; i++) {
    double k = (double)(splitmix() % 20000000) - 10000000.0;
    double scale = pow(10.0, (int)(splitmix() % 16) - 8);
    vals.push_back(k * scale);
  }
  for (int i = 0; i < 700; i++) {
    double a = (double)(splitmix() % 100000) + 1.0;
    double b = (double)(splitmix() % 1000) + 1.0;
    vals.push_back(a / b);
  }
  // Exact half-way cases at several precisions (e.g. x.xxxxx5).
  for (int i = 0; i < 300; i++) {
    double m = (double)(splitmix() % 2000000);
    vals.push_back((m + 0.5) / pow(2.0, (int)(splitmix() % 12)));
  }

  char buf[512];
  for (size_t i = 0; i < vals.size(); i++) {
    const double v = vals[i];
    unsigned long long u;
    memcpy(&u, &v, sizeof(u));
    printf("%016llx", u);
    for (int f = 0; f < NUM_FORMATS; f++) {
      const char* fmt = FORMATS[f];
      if (strchr(fmt, 'f') && std::isfinite(v) && fabs(v) >= 1e22) { printf("\t-"); continue; }
      snprintf(buf, sizeof(buf), fmt, v);
      printf("\t%s", buf);
    }
    std::ostringstream a, b, c;
    a << v;
    b.setf(std::ios::showpoint);
    b << std::setprecision(7) << v;
    c.setf(std::ios::left);
    c.setf(std::ios::showpoint);
    c << std::setw(9) << std::setprecision(7) << v << "|";
    printf("\t%s\t%s\t%s\n", a.str().c_str(), b.str().c_str(), c.str().c_str());
  }
  return 0;
}
