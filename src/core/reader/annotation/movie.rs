//! Movie 注解专有条目读取（12.5.6.17 Table 186）。
//!
//! Movie 注解（PDF 1.2）的 `/Movie` 是电影字典（13.4 Table 295），必填；
//! `/T` 标题与通用注解条目共用 [`super::Annotation::title`]；`/A` 在该
//! 子类型下不是动作，而是布尔或电影激活字典（Table 296），表示注解被
//! 激活时是否以及如何播放电影，缺省 `true`。
//!
//! 13.4 声明电影特性已被 13.2 的多媒体框架取代（obsolescent），本模块
//! 只做语法层类型化，不解释电影文件内容。

use crate::core::reader::movie::{Movie, MoviePlayback, parse_movie_activation};
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{AnnotationSubtype, Result, malformed};

/// Movie 注解的专有条目。
#[derive(Debug, Default)]
pub(super) struct MovieEntries {
    /// `/Movie` 电影字典（Movie 子类型必填）。
    pub(super) movie: Option<Movie>,
    /// `/A` 播放设置；缺失表示按缺省 `true` 处理。
    pub(super) playback: Option<MoviePlayback>,
}

/// 解析 Movie 注解的 `/Movie` 与 `/A`。
///
/// 非 Movie 子类型出现 `/Movie` 时报错；Movie 子类型缺少 `/Movie`、
/// 电影字典损坏、`/A` 既非布尔也非字典时报错。
pub(super) fn parse_movie_entries(
    document: &Document,
    dict: &Dict,
    subtype: &AnnotationSubtype,
) -> Result<MovieEntries> {
    let movie_value = dict.get("Movie");
    if *subtype != AnnotationSubtype::Movie {
        return match movie_value {
            Some(_) => Err(malformed(
                "annotation /Movie is specific to Movie annotations",
            )),
            None => Ok(MovieEntries::default()),
        };
    }
    let movie_value =
        movie_value.ok_or_else(|| malformed("Movie annotation is missing required /Movie"))?;
    let movie = document
        .movie(movie_value)
        .map_err(|error| malformed(format!("Movie annotation /Movie: {error}")))?;
    let playback = match dict.get("A") {
        None => None,
        Some(value) => Some(match resolve(document, value)? {
            Value::Bool(flag) => MoviePlayback::Flag(flag),
            Value::Dict(activation) => MoviePlayback::Activation(
                parse_movie_activation(document, &activation)
                    .map_err(|error| malformed(format!("Movie annotation /A: {error}")))?,
            ),
            _ => {
                return Err(malformed(
                    "Movie annotation /A must be a boolean or an activation dictionary",
                ));
            }
        }),
    };
    Ok(MovieEntries {
        movie: Some(movie),
        playback,
    })
}
