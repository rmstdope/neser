#!/usr/bin/env python3
"""Find K and N values for boot ROM fine-tune delay that give DIV=$AB and LY=$0A."""

import math


def extended_gcd(a, b):
    if b == 0:
        return a, 1, 0
    g, x1, y1 = extended_gcd(b, a % b)
    return g, y1, x1 - (a // b) * y1


def solve_crt(r1, m1, r2, m2):
    g, p, _q = extended_gcd(m1, m2)
    if (r2 - r1) % g != 0:
        return None
    lcm_val = m1 * m2 // g
    sol = (r1 + m1 * ((r2 - r1) // g) * p) % lcm_val
    return sol


# From empirical measurement: K=1, N=60510 → DIV=$A0, LY=$0A
# At K=1, N=60000, hold=32: total_M = 5134835
# Each N adds 7 M-cycles, so at N=60510: total ≈ 5134835 + 7*510 = 5138405
# B-loop adds 4K+1 M, HL-loop adds 7N+2 M, overhead 0 M
# Total = C + (4K+1) + (7N+2) where C is the fixed cost from main loop
# At K=1, N=60510: total = C + 5 + 423572 = C + 423577

# DIV = floor(total_T / 256) & 0xFF = floor(4*total_M / 256) & 0xFF = floor(total_M/64) & 0xFF
# DIV=$A0 means total_M mod 16384 ∈ [0xA0*64, 0xA0*64+63] = [10240, 10303]

# LY = scanline counter. total_M mod 17556 determines LY.
# LY=$0A window is empirically ~106 M-cycles wide.

# Approach: Search D = 4*(K-1) + 7*(N-60510) such that:
#   (T0 + D) mod 16384 ∈ [0xAB*64, 0xAB*64+63]  (DIV=$AB)
#   (T0 + D) mod 17556 ∈ [T0%17556 - 53, T0%17556 + 53]  (LY stays $0A)

# We don't know exact T0, but T0 % 16384 ∈ [10240, 10303].
# Use the measurement: at N=60510, K=1, DIV=$A0 → let's assume T0%16384 = 10270 (estimated).
# And T0 % 17556 is in the LY=$0A window center.

# More robust: search all (K, N) where D = 4*(K-1) + 7*(N-60510)
# and D mod 16384 ∈ [target_div_lo - t0_mod, target_div_hi - t0_mod]
# and D mod 17556 ∈ [-53, 53]

# From data: at K=12, N=60500: DIV=$9F, LY=$0A. At K=39, N=60500: DIV=$A1, LY=$0A.
# LY=$0A window at N=60500 spans K=12..39 = 28 K = 112 M-cycles = 112 mod 17556 shift.

# Key: D % 16384 must be in [target_range] and D % 17556 must be near 0.
# Since gcd(16384, 17556) = 4, CRT period = lcm = 71917056.

# Instead of CRT, just iterate over reasonable (K, N) and check.
# K ∈ [1, 255], N ∈ [1, 65535]
# D = 4*(K-1) + 7*(N-60510)

# For DIV=$AB: need (T0 + D) / 64 & 0xFF == 0xAB
#   T0/64 & 0xFF = 0xA0, so (T0+D)/64 & 0xFF = 0xAB
#   D/64 & 0xFF must shift by $0B = 11
#   D mod 16384 ∈ [11*64, 11*64+63] = [704, 767]
#   BUT this ignores the exact T0 value within its 64-value range.
#   More precisely: D mod 16384 ∈ [704 - (T0%64), 767 - (T0%64) + 63]
#   Since T0%64 is unknown (0-63), D mod 16384 ∈ [704-63, 767+0] = [641, 767]

# For LY=$0A: D mod 17556 ∈ [-53, 53] (to stay within the ~106 M-cycle window)

# Search D = 16384*a + r where r ∈ [641, 767] and D % 17556 ∈ [-53, 53]
# D can be decomposed as 4*(K-1) + 7*(N-60510)

# First find valid D values
print("Searching for valid D values with variable hold count...")

# Each hold iteration adds ~35113 M-cycles.
# Changing hold by 1 shifts total by ±35113, changing D by ±35113.
# So for hold H (vs baseline 32): D_offset = (H-32) * 35113
# New D = D_offset + 4*(K-1) + 7*(N-60510)
# Need: (T0 + D) mod 16384 ∈ [10944, 11007]  (DIV=$AB)
# Need: (T0 + D) mod 17556 in LY=$0A window

# T0 is at hold=32, K=1, N=60510
# T0 mod 16384 ∈ [10240, 10303] (DIV=$A0)
# T0 mod 17556 = center of LY=$0A window

# For different holds, the offset changes:
# D_total = 35113*(H-32) + 4*(K-1) + 7*(N-60510)
# Need D_total mod 16384 ∈ [704, 767]  (DIV shift from $A0 to $AB)
# Need D_total mod 17556 ∈ [-53, 53]

# 35113 mod 16384 = 35113 - 2*16384 = 2345
# 35113 mod 17556 = 35113 - 17556 = 17557. Wait: 35113 - 17556 = 17557. 17557 - 17556 = 1.
# So 35113 mod 17556 = 1!

print(f"35113 mod 16384 = {35113 % 16384}")
print(f"35113 mod 17556 = {35113 % 17556}")

# 35113 mod 17556 = 1! This means each hold iteration shifts LY by just 1 M-cycle.
# After H-32 extra hold iterations, the LY shift is (H-32) mod 17556.
# And the DIV shift is ((H-32)*2345) mod 16384.

# So for hold H:
# D_total mod 16384 = (H-32)*2345 + 4*(K-1) + 7*(N-60510)) mod 16384
# D_total mod 17556 = (H-32)*1 + 4*(K-1) + 7*(N-60510)) mod 17556

# Need D_total mod 16384 ∈ [704, 767]
# Need D_total mod 17556 ∈ [-53, 53]

# Since hold change barely affects LY (1 M per hold), the N range stays similar.
# The DIV shift from hold change: (H-32)*2345 mod 16384.

# We need: ((H-32)*2345 + 4*(K-1) + 7*(N-60510)) mod 16384 ∈ [704, 767]
# So: (4*(K-1) + 7*(N-60510)) mod 16384 ∈ [704 - (H-32)*2345, 767 - (H-32)*2345] (mod 16384)

# And for LY: (H-32 + 4*(K-1) + 7*(N-60510)) mod 17556 ∈ [-53, 53]

# Let X = 4*(K-1) + 7*(N-60510) = 4*dk + 7*dn
# Need X mod 16384 = (704 - (H-32)*2345) mod 16384  (within range)
# Need X mod 17556 = (-(H-32)) mod 17556  (within range ±53)

# For LY: X mod 17556 ≈ -(H-32) ± 53
# For small |H-32|, X mod 17556 is near -(H-32).

# For H=32: X mod 16384 ∈ [704, 767], X mod 17556 ∈ [-53, 53]. (Already proven impossible)
# For H=33: X mod 16384 ∈ [704-2345, 767-2345] mod 16384 = [-1641, -1578] = [14743, 14806]
#           X mod 17556 ∈ [-1-53, -1+53] = [-54, 52]
# For H=31: X mod 16384 ∈ [704+2345, 767+2345] = [3049, 3112]
#           X mod 17556 ∈ [1-53, 1+53] = [-52, 54]

solutions = []
for H in range(20, 48):
    div_offset = ((H - 32) * 2345) % 16384
    ly_offset = H - 32  # each hold adds 1 to LY mod

    target_div_lo = (704 - div_offset) % 16384
    target_div_hi = (767 - div_offset) % 16384

    # Handle wraparound
    if target_div_lo > target_div_hi:
        div_ranges = [(0, target_div_hi), (target_div_lo, 16383)]
    else:
        div_ranges = [(target_div_lo, target_div_hi)]

    target_ly_center = -ly_offset
    target_ly_lo = target_ly_center - 53
    target_ly_hi = target_ly_center + 53

    for div_lo, div_hi in div_ranges:
        for r1 in range(div_lo, div_hi + 1):
            for r2_raw in range(target_ly_lo, target_ly_hi + 1):
                r2 = r2_raw % 17556
                if r1 % 4 != r2 % 4:
                    continue
                X_base = solve_crt(r1, 16384, r2, 17556)
                if X_base is None:
                    continue
                # Try X_base and X_base - lcm_val
                lcm_val = 16384 * 17556 // math.gcd(16384, 17556)
                for X in [X_base, X_base - lcm_val]:
                    for dk in range(0, 255):
                        remainder = X - 4 * dk
                        if remainder % 7 == 0:
                            dn = remainder // 7
                            N = 60510 + dn
                            K = 1 + dk
                            if 1 <= K <= 255 and 1 <= N <= 65535:
                                solutions.append((H, K, N, X, r2_raw))
                                break

solutions.sort(key=lambda x: abs(x[0] - 32))  # prefer hold close to 32
print(f"\nFound {len(solutions)} valid solutions")
for H, K, N, X, ly_raw in solutions[:30]:
    print(f"  hold={H:2d} K={K:3d} N={N:5d} (${N:04X}) X={X:8d} ly_raw={ly_raw:+4d}")
