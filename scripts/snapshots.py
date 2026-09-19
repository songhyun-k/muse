#!/usr/bin/env python3
"""Compare every production-rendered cell against the frozen Korean/English views."""
from reference import REFERENCE, cases, compare, load_reference, render


def main():
    inputs=cases()
    compare(load_reference(REFERENCE/'snapshots.json.gz',inputs),render(inputs))


if __name__ == '__main__': main()
