use groq_format::{format_query, FormatError};

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_blog_post_query() {
        let input = r#"*[_type=="post"&&published==true]{_id,title,slug,author->{name,image{asset->{url}}},publishedAt,excerpt,categories[]->{title,slug}}"#;
        let expected = r#"*[_type == "post" && published == true] {
  _id,
  title,
  slug,
  author-> { name, image { asset-> { url } } },
  publishedAt,
  excerpt,
  categories[]-> { title, slug }
}"#;

        let result = format_query(input, 80).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ecommerce_product_query() {
        let input = r#"*[_type=="product"&&inStock==true]{_id,name,price,images[]{asset->{url}},variants[]{name,price},categories[]->{title,slug},tags}"#;
        let expected = r#"*[_type == "product" && inStock == true] {
  _id,
  name,
  price,
  images[] { asset-> { url } },
  variants[] { name, price },
  categories[]-> { title, slug },
  tags
}"#;

        let result = format_query(input, 60).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_nested_reference_projection() {
        let input = r#"*[_type=="article"]{title,author->{name,bio,image{asset->{url,metadata{dimensions{width,height}}}}},tags[]->{name,slug}}"#;
        let expected = r#"*[_type == "article"] {
  title,
  author-> {
    name,
    bio,
    image {
      asset-> {
        url,
        metadata { dimensions { width, height } }
      }
    }
  },
  tags[]-> { name, slug }
}"#;

        let result = format_query(input, 50).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_pipe_operations() {
        let input = r#"*[_type=="event"] | order(date asc) {title,date,location,price}"#;
        let expected = r#"*[_type == "event"] | order(date asc) {
  title,
  date,
  location,
  price
}"#;

        let result = format_query(input, 40).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_complex_filters() {
        let input = r#"*[_type in["post","article"]&&defined(slug.current)&&published==true&&(!defined(category)||category->slug.current!="draft")]{_id,title,slug,publishedAt}"#;
        let expected = r#"*[_type in ["post", "article"] && defined(slug.current)
  && published == true
  && (!defined(category) || category->slug.current != "draft")] {
  _id,
  title,
  slug,
  publishedAt
}"#;

        let result = format_query(input, 70).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_array_operations() {
        let input = r#"*[_type=="portfolio"]{title,images[0..5]{asset->{url}},skills[defined(name)]{name,level},projects[]{name,url,status}}"#;
        let expected = r#"*[_type == "portfolio"] {
  title,
  images[0..5] { asset-> { url } },
  skills[defined(name)] { name, level },
  projects[] { name, url, status }
}"#;

        let result = format_query(input, 60).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_conditional_expressions() {
        let input = r#"*[_type=="user"]{name,role,permissions[]{name,granted},isAdmin==true=>{"admin":true,"level":"super"},isAdmin==false=>{"admin":false,"level":"user"}}"#;
        let expected = r#"*[_type == "user"] {
  name,
  role,
  permissions[] { name, granted },
  isAdmin == true => { "admin": true, "level": "super" },
  isAdmin == false => { "admin": false, "level": "user" }
}"#;

        let result = format_query(input, 80).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_order_and_slice() {
        let input = r#"*[_type=="review"&&rating>=4] | order(_createdAt desc) [0..10] {_id,rating,comment,customer->{name},product->{title}}"#;
        let expected = r#"*[_type == "review" && rating >= 4] | order(_createdAt desc)[0..10] {
  _id,
  rating,
  comment,
  customer-> { name },
  product-> { title }
}"#;

        let result = format_query(input, 70).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_function_calls() {
        let input = r#"*[_type=="article"]{title,slug,upper(title),lower(excerpt),string::split(tags,",")}"#;
        let expected = r#"*[_type == "article"] {
  title,
  slug,
  upper(title),
  lower(excerpt),
  string::split(tags, ",")
}"#;

        let result = format_query(input, 60).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_empty_structures() {
        let input = r#"*[_type=="test"]{emptyArray:[],emptyObject:{},normalField}"#;
        let expected = r#"*[_type == "test"] { emptyArray: [], emptyObject: {}, normalField }"#;

        let result = format_query(input, 80).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_unicode_and_special_chars() {
        let input = r#"*[_type=="test"]{title:"Hello 🌍 World",description:"café résumé naïve"}"#;
        let expected = r#"*[_type == "test"] {
  title: "Hello 🌍 World",
  description: "café résumé naïve"
}"#;

        let result = format_query(input, 80).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_malformed_queries() {
        let malformed_queries = vec![
            r#"*[_type=="test" &&"#,
            r#"*[_type=="test"]{field"#,
            r#"*[_type=="test"]{field:}"#,
            r#"function_call("#,
        ];

        for query in malformed_queries {
            let result = format_query(query, 80);
            assert!(
                matches!(result, Err(FormatError::Parse(_))),
                "Query should fail: {}",
                query
            );
        }
    }

    #[test]
    fn test_line_wrapping_narrow_width() {
        let input = r#"*[_type=="test"]{field1,field2,field3,field4,field5}"#;
        let expected = r#"*[_type == "test"] {
  field1,
  field2,
  field3,
  field4,
  field5
}"#;

        let result = format_query(input, 20).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_line_wrapping_complex_conditions() {
        let input = r#"*[_type=="test"&&condition1&&condition2&&condition3]"#;
        let expected = r#"*[_type == "test"
  && condition1
  && condition2
  && condition3]"#;

        let result = format_query(input, 30).unwrap();
        assert_eq!(result, expected);
    }
}
