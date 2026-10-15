pub(super) fn renderer_function(name: &str) -> bool {
    name.starts_with("lince_")
        || name.contains("_naga_oil_")
        || NAMES
            .split_ascii_whitespace()
            .any(|reserved| reserved == name)
}

const NAMES: &str = "
abs acos acosh all any arrayLength asin asinh atan atanh atan2 ceil clamp cos cosh
countLeadingZeros countOneBits countTrailingZeros cross degrees determinant distance dot
dot4I8Packed dot4U8Packed dpdx dpdxCoarse dpdxFine dpdy dpdyCoarse dpdyFine exp exp2
extractBits faceForward firstLeadingBit firstTrailingBit floor fma fract frexp fwidth
fwidthCoarse fwidthFine insertBits inverseSqrt ldexp length log log2 max min mix modf
normalize pow quantizeToF16 radians reflect refract reverseBits round saturate select sign
sin sinh smoothstep sqrt step tan tanh transpose trunc
pack4x8snorm pack4x8unorm pack2x16snorm pack2x16unorm pack2x16float
pack4xI8 pack4xU8 pack4xI8Clamp pack4xU8Clamp unpack4x8snorm unpack4x8unorm
unpack2x16snorm unpack2x16unorm unpack2x16float unpack4xI8 unpack4xU8
textureDimensions textureGather textureGatherCompare textureLoad textureNumLayers
textureNumLevels textureNumSamples textureSample textureSampleBias textureSampleCompare
textureSampleCompareLevel textureSampleGrad textureSampleLevel textureSampleBaseClampToEdge textureStore
atomicLoad atomicStore atomicAdd atomicSub atomicMax atomicMin atomicAnd atomicOr atomicXor
atomicExchange atomicCompareExchangeWeak workgroupBarrier storageBarrier textureBarrier workgroupUniformLoad
subgroupAdd subgroupAnd subgroupAll subgroupAny subgroupBallot subgroupBroadcast subgroupBroadcastFirst
subgroupElect subgroupExclusiveAdd subgroupExclusiveMul subgroupInclusiveAdd subgroupInclusiveMul
subgroupMax subgroupMin subgroupMul subgroupOr subgroupShuffle subgroupShuffleDown subgroupShuffleUp
subgroupShuffleXor subgroupXor quadBroadcast quadSwapX quadSwapY quadSwapDiagonal
get_world_from_local get_local_from_world mesh_position_local_to_clip mesh_position_local_to_world
mesh_normal_local_to_world prepass_depth frag_coord_to_ndc position_ndc_to_world
";
