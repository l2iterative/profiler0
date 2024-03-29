use core::mem::{transmute, MaybeUninit};
use risc0_zkvm::guest::env;
use l2r0_profiler_guest::*;

pub(crate) const BIGINT_WIDTH_WORDS: usize = 8;
const OP_MULTIPLY: u32 = 0;

const TEST_MODULUS: [u32; 8] = [
    4294967107u32,
    4294967295u32,
    4294967295u32,
    4294967295u32,
    4294967295u32,
    4294967295u32,
    4294967295u32,
    4294967295u32,
];

const N_LIMBS: [u32; 66] = [
    3493812455u32,
    3529997461u32,
    710143587u32,
    2792692495u32,
    1885047707u32,
    3553628773u32,
    2204079629u32,
    699911535u32,
    3275286756u32,
    2670964040u32,
    380836659u32,
    1539088076u32,
    257233178u32,
    102057303u32,
    3498423094u32,
    347591143u32,
    118634769u32,
    2922120165u32,
    4044052678u32,
    3306267357u32,
    3299705609u32,
    2232715160u32,
    2567218027u32,
    57867452u32,
    3266166781u32,
    2351768864u32,
    296981719u32,
    1570354344u32,
    4098249795u32,
    2000361393u32,
    1479034620u32,
    3336008768u32,
    2938032753u32,
    3528598023u32,
    1304193507u32,
    121827407u32,
    514584826u32,
    1603753032u32,
    1664712145u32,
    3527467765u32,
    2821704060u32,
    729040642u32,
    2110748820u32,
    3709644666u32,
    4149792411u32,
    1565350608u32,
    3206857463u32,
    792901230u32,
    3569404149u32,
    1620994961u32,
    33783729u32,
    1281610576u32,
    468794176u32,
    1193160222u32,
    3636051391u32,
    2450661453u32,
    4242348214u32,
    2150858390u32,
    1813504491u32,
    305305593u32,
    1673370015u32,
    1864962247u32,
    2629885700u32,
    2947918631u32,
    0u32,
    0u32,
];

extern "C" {
    fn sys_bigint(
        result: *mut [u32; BIGINT_WIDTH_WORDS],
        op: u32,
        x: *const [u32; BIGINT_WIDTH_WORDS],
        y: *const [u32; BIGINT_WIDTH_WORDS],
        modulus: *const [u32; BIGINT_WIDTH_WORDS],
    );
}

#[inline]
pub fn add32_and_overflow(a: u32, b: u32, carry: u32) -> (u32, u32) {
    let v = (a as u64).wrapping_add(b as u64).wrapping_add(carry as u64);
    ((v >> 32) as u32, (v & 0xffffffff) as u32)
}
#[inline]
pub fn add_small<const I: usize, const J: usize>(accm: &mut [u32; I], new: &[u32; J]) {
    let mut carry = 0;
    (carry, accm[0]) = add32_and_overflow(accm[0], new[0], carry);
    for i in 1..J {
        (carry, accm[i]) = add32_and_overflow(accm[i], new[i], carry);
    }
    for i in J..I {
        (carry, accm[i]) = add32_and_overflow(accm[i], carry, 0);
    }
}

pub struct Task {
    // 22 limbs, each of length 96 bits = 3 x u32 = 12 x u8
    pub a: [u8; 264],
    // 22 limbs, each of length 96 bits = 3 x u32 = 12 x u8
    pub b: [u8; 264],
    // 43 limbs, each of length 224 bits = 7 x u32 = 28 x u8
    // total: 1204 bytes
    pub long_form_c: [u8; 1204],
    // 22 limbs, each of length 96 bits = 3 x u32 = 12 x u8
    // total: 264 bytes
    pub k: [u8; 264],
    // 43 limbs, each of length 224 bits = 7 x u32 = 28 x u8
    // total: 1204 bytes
    pub long_form_kn: [u8; 1204],
}

#[inline]
pub fn sub_with_borrow(a: u32, b: u32, carry: u32) -> (u32, u32) {
    let res = ((a as u64).wrapping_add(0x100000000))
        .wrapping_sub(b as u64)
        .wrapping_sub(carry as u64);
    (
        (res & 0xffffffff) as u32,
        1u32.wrapping_sub((res >> 32) as u32),
    )
}

#[inline]
pub fn sub_and_borrow<const I: usize>(accu: &mut [u32; I], new: &[u32; I]) -> u32 {
    let (cur, borrow) = accu[0].overflowing_sub(new[0]);
    accu[0] = cur;

    let mut borrow = borrow as u32;
    for i in 1..I - 1 {
        (accu[i], borrow) = sub_with_borrow(accu[i], new[i], borrow);
    }
    (accu[I - 1], borrow) = sub_with_borrow(accu[I - 1], new[I - 1], borrow);
    borrow
}

fn compute_checksum_small_and_reduce(ptr: &[u32; 66], z: &[[u32; 8]; 43]) -> [u32; 8] {
    let mut res = [0u32; 8];
    let mut checksum = [0u32; 9];

    for i in 0..22 {
        let tmp_limbs = [ptr[i * 3], ptr[i * 3 + 1], ptr[i * 3 + 2], 0u32, 0, 0, 0, 0];

        unsafe {
            sys_bigint(
                &mut res as *mut [u32; BIGINT_WIDTH_WORDS],
                OP_MULTIPLY,
                &tmp_limbs as *const [u32; BIGINT_WIDTH_WORDS],
                &z[i] as *const [u32; BIGINT_WIDTH_WORDS],
                &TEST_MODULUS,
            );
        }
        add_small::<9, 8>(&mut checksum, &res);
    }
    while checksum[8] != 0 {
        let reducer = [checksum[8], 0, 0, 0, 0, 0, 0, 0];
        checksum[8] = 0;

        unsafe {
            sys_bigint(
                &mut res as *mut [u32; BIGINT_WIDTH_WORDS],
                OP_MULTIPLY,
                &reducer as *const [u32; BIGINT_WIDTH_WORDS],
                &[189u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32],
                &TEST_MODULUS,
            );
        }
        unsafe {
            add_small::<9, 2>(&mut checksum, &transmute::<&[u32; 8], &[u32; 2]>(&res));
        }
    }

    let mut res = [0u32; 8];
    res.copy_from_slice(&checksum[0..8]);
    res
}

fn compute_checksum_long_and_reduce(ptr: &[u32; 301], z: &[[u32; 8]; 43]) -> [u32; 8] {
    let mut res = [0u32; 8];
    let mut checksum = [0u32; 9];

    for i in 0..43 {
        let tmp_limbs = [
            ptr[i * 7],
            ptr[i * 7 + 1],
            ptr[i * 7 + 2],
            ptr[i * 7 + 3],
            ptr[i * 7 + 4],
            ptr[i * 7 + 5],
            ptr[i * 7 + 6],
            0,
        ];

        unsafe {
            sys_bigint(
                &mut res as *mut [u32; BIGINT_WIDTH_WORDS],
                OP_MULTIPLY,
                &tmp_limbs as *const [u32; BIGINT_WIDTH_WORDS],
                &z[i] as *const [u32; BIGINT_WIDTH_WORDS],
                &TEST_MODULUS,
            );
        }
        add_small::<9, 8>(&mut checksum, &res);
    }
    while checksum[8] != 0 {
        let reducer = [checksum[8], 0, 0, 0, 0, 0, 0, 0];
        checksum[8] = 0;

        unsafe {
            sys_bigint(
                &mut res as *mut [u32; BIGINT_WIDTH_WORDS],
                OP_MULTIPLY,
                &reducer as *const [u32; BIGINT_WIDTH_WORDS],
                &[189u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32],
                &TEST_MODULUS,
            );
        }
        unsafe {
            add_small::<9, 2>(&mut checksum, &transmute::<&[u32; 8], &[u32; 2]>(&res));
        }
    }

    let mut res = [0u32; 8];
    res.copy_from_slice(&checksum[0..8]);
    res
}

fn main() {
    l2r0_profiler_guest::init_trace_logger();
    start_timer!("Total");

    /************************************************************/
    start_timer!("Load data");
    start_timer!("Read from the host");
    // read the task and the witness from the host
    let mut task = MaybeUninit::<Task>::uninit();
    unsafe {
        env::read_slice(&mut (*task.as_mut_ptr()).a);
        env::read_slice(&mut (*task.as_mut_ptr()).b);
        env::read_slice(&mut (*task.as_mut_ptr()).long_form_c);
        env::read_slice(&mut (*task.as_mut_ptr()).k);
        env::read_slice(&mut (*task.as_mut_ptr()).long_form_kn);
    }
    let task = unsafe { task.assume_init() };
    /************************************************************/

    stop_start_timer!("Check the length");
    // check the length
    assert_eq!(task.long_form_c.len(), 1204);
    assert_eq!(task.k.len(), 264);
    assert_eq!(task.long_form_kn.len(), 1204);

    /************************************************************/
    stop_start_timer!("Hash");
    // derive the challenge
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"RISC Zero RSA Gadget");
    hasher.update(&task.a);
    hasher.update(&task.b);
    hasher.update(&task.long_form_c);
    hasher.update(&task.k);
    hasher.update(&task.long_form_kn);

    let final_hash = hasher.finalize().to_vec();
    stop_timer!();
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Compute z");
    let mut z = MaybeUninit::<[[u32; 8]; 43]>::uninit();
    unsafe {
        (*z.as_mut_ptr())[0] = [1u32, 0, 0, 0, 0, 0, 0, 0];
        (*z.as_mut_ptr())[1].copy_from_slice(transmute::<&u8, &[u32; 8]>(&final_hash[0]));
    }

    for i in 2..43 {
        unsafe {
            sys_bigint(
                &mut (*z.as_mut_ptr())[i] as *mut [u32; BIGINT_WIDTH_WORDS],
                OP_MULTIPLY,
                &(*z.as_ptr())[i - 1] as *const [u32; BIGINT_WIDTH_WORDS],
                &(*z.as_ptr())[1] as *const [u32; BIGINT_WIDTH_WORDS],
                &TEST_MODULUS,
            );
        }
    }

    let z = unsafe { z.assume_init() };
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Compute and reduce the checksum for a, b, k, n");
    start_timer!("Compute and reduce the checksum for a");
    let az_reduce =
        compute_checksum_small_and_reduce(unsafe { transmute::<&u8, &[u32; 66]>(&task.a[0]) }, &z);
    stop_start_timer!("Compute and reduce the checksum for b");
    let bz_reduce =
        compute_checksum_small_and_reduce(unsafe { transmute::<&u8, &[u32; 66]>(&task.b[0]) }, &z);
    stop_start_timer!("Compute and reduce the checksum for k");
    let kz_reduce =
        compute_checksum_small_and_reduce(unsafe { transmute::<&u8, &[u32; 66]>(&task.k[0]) }, &z);
    stop_start_timer!("Compute and reduce the checksum for n");
    let nz_reduce = compute_checksum_small_and_reduce(
        unsafe { transmute::<&u32, &[u32; 66]>(&N_LIMBS[0]) },
        &z,
    );
    stop_timer!();
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Compute and reduce the checksum for c, kn");
    start_timer!("Compute and reduce the checksum for c");
    let cz_reduce = compute_checksum_long_and_reduce(
        unsafe { transmute::<&u8, &[u32; 301]>(&task.long_form_c[0]) },
        &z,
    );
    stop_start_timer!("Compute and reduce the checksum for kn");
    let knz_reduce = compute_checksum_long_and_reduce(
        unsafe { transmute::<&u8, &[u32; 301]>(&task.long_form_kn[0]) },
        &z,
    );
    stop_timer!();
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Compute a(z) * b(z), k(z) * n(z)");
    start_timer!("Compute a(z) * b(z)");
    let mut az_times_bz = [0u32; 8];
    unsafe {
        sys_bigint(
            &mut az_times_bz as *mut [u32; BIGINT_WIDTH_WORDS],
            OP_MULTIPLY,
            &az_reduce as *const [u32; BIGINT_WIDTH_WORDS],
            &bz_reduce as *const [u32; BIGINT_WIDTH_WORDS],
            &TEST_MODULUS,
        );
    }

    stop_start_timer!("Compute k(z) * n(z)");
    let mut kz_times_nz = [0u32; 8];
    unsafe {
        sys_bigint(
            &mut kz_times_nz as *mut [u32; BIGINT_WIDTH_WORDS],
            OP_MULTIPLY,
            &kz_reduce as *const [u32; BIGINT_WIDTH_WORDS],
            &nz_reduce as *const [u32; BIGINT_WIDTH_WORDS],
            &TEST_MODULUS,
        );
    }

    stop_start_timer!("Compare a(z) * b(z) and c(z)");
    assert_eq!(az_times_bz, cz_reduce[0..8]);

    stop_start_timer!("Compare k(z) * n(z) and kn(z)");
    assert_eq!(kz_times_nz, knz_reduce[0..8]);

    stop_timer!();
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Reduce c and kn");

    let mut c_reduce_limbs = [0u32; 129];
    let mut kn_reduce_limbs = [0u32; 129];

    let c_ptr = unsafe { transmute::<&u8, &[u32; 301]>(&task.long_form_c[0]) };
    let kn_ptr = unsafe { transmute::<&u8, &[u32; 301]>(&task.long_form_kn[0]) };

    start_timer!("Reduce c");
    let mut carry = [0u32; 5];
    for i in 0..43 {
        let mut cur_limb = [
            c_ptr[i * 7],
            c_ptr[i * 7 + 1],
            c_ptr[i * 7 + 2],
            c_ptr[i * 7 + 3],
            c_ptr[i * 7 + 4],
            c_ptr[i * 7 + 5],
            c_ptr[i * 7 + 6],
            0,
        ];
        if i != 0 {
            add_small::<8, 5>(&mut cur_limb, &carry);
        }
        carry[0] = cur_limb[3];
        carry[1] = cur_limb[4];
        carry[2] = cur_limb[5];
        carry[3] = cur_limb[6];
        carry[4] = cur_limb[7];

        c_reduce_limbs[i * 3] = cur_limb[0];
        c_reduce_limbs[i * 3 + 1] = cur_limb[1];
        c_reduce_limbs[i * 3 + 2] = cur_limb[2];
    }

    stop_start_timer!("Reduce k");

    let mut carry = [0u32; 5];
    for i in 0..43 {
        let mut cur_limb = [
            kn_ptr[i * 7],
            kn_ptr[i * 7 + 1],
            kn_ptr[i * 7 + 2],
            kn_ptr[i * 7 + 3],
            kn_ptr[i * 7 + 4],
            kn_ptr[i * 7 + 5],
            kn_ptr[i * 7 + 6],
            0,
        ];
        if i != 0 {
            add_small::<8, 5>(&mut cur_limb, &carry);
        }
        carry[0] = cur_limb[3];
        carry[1] = cur_limb[4];
        carry[2] = cur_limb[5];
        carry[3] = cur_limb[6];
        carry[4] = cur_limb[7];

        kn_reduce_limbs[i * 3] = cur_limb[0];
        kn_reduce_limbs[i * 3 + 1] = cur_limb[1];
        kn_reduce_limbs[i * 3 + 2] = cur_limb[2];
    }
    stop_timer!();
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Subtract kn from c");
    let borrow = sub_and_borrow::<129>(&mut c_reduce_limbs, &kn_reduce_limbs);
    stop_timer!();
    /************************************************************/

    /************************************************************/
    start_timer!("Perform final reduction");
    let mut ok_flag = borrow == 0;
    for i in 64..129 {
        ok_flag &= c_reduce_limbs[i] == 0;
    }
    assert!(ok_flag);

    let mut u = [0u32; 64];
    let mut borrow = 0u32;
    for i in 0..64 {
        let res = ((c_reduce_limbs[i] as u64).wrapping_add(0x100000000))
            .wrapping_sub(N_LIMBS[i] as u64)
            .wrapping_sub(borrow as u64);
        u[i] = (res & 0xffffffff) as u32;
        borrow = 1u32.wrapping_sub((res >> 32) as u32);
    }

    // t > n
    if borrow == 0 {
        for i in 0..64 {
            c_reduce_limbs[i] = u[i];
        }
    }
    stop_timer!();
    /************************************************************/

    stop_timer!();
}
