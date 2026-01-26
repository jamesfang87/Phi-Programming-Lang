#include "Driver/PhiBuildSystem.hpp"

int main(int argc, char *argv[]) {
  phi::PhiBuildSystem(argc, argv).build();
  return 0;
}
